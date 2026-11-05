use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use argon2::PasswordVerifier;
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{constants::codes::CODE_UNAUTHORIZED, pkg::errors::AppError};

/// Derives a consistent 32-byte AES-256 key from any input string.
/// If valid 64-char hex, decodes to 32 bytes; otherwise hashes with SHA-256.
pub fn derive_master_key(input: &str) -> [u8; 32] {
    let trimmed = input.trim();
    if trimmed.len() == 64 {
        if let Ok(bytes) = hex::decode(trimmed) {
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                return key;
            }
        }
    }
    // Fallback: SHA-256 hash of the master key string gives 32 bytes
    let hash = Sha256::digest(trimmed.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&hash);
    key
}

/// Encrypts plaintext using AES-256-GCM.
/// Returns (encrypted_value_hex, iv_hex, auth_tag_hex).
pub fn encrypt_secret(
    plaintext: &str,
    master_key: &[u8; 32],
) -> Result<(String, String, String), AppError> {
    let key = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);

    // 12-byte Initialization Vector (IV)
    let mut iv_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv_bytes);
    let nonce = Nonce::from_slice(&iv_bytes);

    let ciphertext_with_tag = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| AppError::internal(format!("Encryption error: {}", e)))?;

    if ciphertext_with_tag.len() < 16 {
        return Err(AppError::internal("Invalid ciphertext length".to_string()));
    }

    let split_pos = ciphertext_with_tag.len() - 16;
    let ciphertext = &ciphertext_with_tag[..split_pos];
    let auth_tag = &ciphertext_with_tag[split_pos..];

    Ok((
        hex::encode(ciphertext),
        hex::encode(iv_bytes),
        hex::encode(auth_tag),
    ))
}

/// Decrypts ciphertext using AES-256-GCM with IV and auth tag verification.
pub fn decrypt_secret(
    encrypted_value_hex: &str,
    iv_hex: &str,
    auth_tag_hex: &str,
    master_key: &[u8; 32],
) -> Result<String, AppError> {
    let ciphertext = hex::decode(encrypted_value_hex)
        .map_err(|e| AppError::bad_request(format!("Invalid ciphertext hex: {}", e), "INVALID_HEX"))?;
    let iv_bytes = hex::decode(iv_hex)
        .map_err(|e| AppError::bad_request(format!("Invalid IV hex: {}", e), "INVALID_IV"))?;
    let auth_tag = hex::decode(auth_tag_hex)
        .map_err(|e| AppError::bad_request(format!("Invalid auth tag hex: {}", e), "INVALID_TAG"))?;

    if iv_bytes.len() != 12 {
        return Err(AppError::bad_request("IV must be exactly 12 bytes", "INVALID_IV_LENGTH"));
    }
    if auth_tag.len() != 16 {
        return Err(AppError::bad_request("Auth tag must be exactly 16 bytes", "INVALID_TAG_LENGTH"));
    }

    let key = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&iv_bytes);

    // Reconstruct ciphertext + tag buffer expected by aes-gcm
    let mut full_payload = Vec::with_capacity(ciphertext.len() + auth_tag.len());
    full_payload.extend_from_slice(&ciphertext);
    full_payload.extend_from_slice(&auth_tag);

    let decrypted_bytes = cipher
        .decrypt(nonce, full_payload.as_ref())
        .map_err(|_| AppError::bad_request("Decryption failed: authentication tag mismatch or corrupted data", "DECRYPTION_FAILED"))?;

    String::from_utf8(decrypted_bytes)
        .map_err(|e| AppError::internal(format!("Decrypted data is not valid UTF-8: {}", e)))
}

/// Generates a new machine/service token:
/// Returns (raw_token, token_prefix, token_hash)
/// raw_token format: `exv_<32_random_hex_chars>`
pub fn generate_service_token() -> (String, String, String) {
    let mut rand_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut rand_bytes);
    let hex_part = hex::encode(rand_bytes); // 32 hex chars
    let raw_token = format!("exv_{}", hex_part);
    let token_prefix = format!("exv_{}...", &hex_part[..4]);
    let token_hash = hash_service_token(&raw_token);

    (raw_token, token_prefix, token_hash)
}

/// Computes the SHA-256 hex digest of a service token.
pub fn hash_service_token(raw_token: &str) -> String {
    let hash = Sha256::digest(raw_token.as_bytes());
    hex::encode(hash)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub username: String,
    pub email: String,
    pub role: String,
    pub exp: usize,
    pub iat: usize,
}

/// Creates a signed HS256 JWT valid for 7 days.
pub fn create_jwt(user_id: &str, username: &str, email: &str, role: &str, secret: &str) -> Result<String, AppError> {
    let now = Utc::now().timestamp() as usize;
    let exp = now + (7 * 24 * 60 * 60); // 7 days

    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        exp,
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::internal(format!("JWT creation error: {}", e)))
}

/// Verifies a signed JWT token.
pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims, AppError> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::unauthorized("Invalid or expired session token", CODE_UNAUTHORIZED))?;

    Ok(token_data.claims)
}

/// Validates admin password against plaintext or argon2 hash.
pub fn verify_admin_password(input_password: &str, stored_password_or_hash: &str) -> bool {
    // If argon2 hash
    if stored_password_or_hash.starts_with("$argon2") {
        if let Ok(parsed_hash) = argon2::PasswordHash::new(stored_password_or_hash) {
            return argon2::Argon2::default()
                .verify_password(input_password.as_bytes(), &parsed_hash)
                .is_ok();
        }
    }
    // Otherwise direct comparison
    input_password == stored_password_or_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let key = derive_master_key("my-super-secret-passphrase");
        let plaintext = "DB_PASSWORD=super_secret_p@ssw0rd!#123\nMULTILINE=\"true\"";

        let (encrypted, iv, tag) = encrypt_secret(plaintext, &key).expect("Encryption failed");
        assert_ne!(plaintext, encrypted);
        assert_eq!(iv.len(), 24); // 12 bytes = 24 hex
        assert_eq!(tag.len(), 32); // 16 bytes = 32 hex

        let decrypted = decrypt_secret(&encrypted, &iv, &tag, &key).expect("Decryption failed");
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_tampered_tag_fails() {
        let key = derive_master_key("test_key");
        let (encrypted, iv, tag) = encrypt_secret("sensitive_data", &key).unwrap();

        // Tamper with tag
        let mut tampered_tag = tag;
        tampered_tag.replace_range(0..2, "ff");

        let result = decrypt_secret(&encrypted, &iv, &tampered_tag, &key);
        assert!(result.is_err());
    }

    #[test]
    fn test_service_token_format_and_hashing() {
        let (raw, prefix, hash) = generate_service_token();
        assert!(raw.starts_with("exv_"));
        assert_eq!(raw.len(), 36); // "exv_" (4) + 32 hex chars
        assert!(prefix.starts_with("exv_"));
        assert_eq!(hash.len(), 64); // SHA-256 hex is 64 chars

        let computed_hash = hash_service_token(&raw);
        assert_eq!(hash, computed_hash);
    }
}
