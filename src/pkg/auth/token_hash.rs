use sha2::{Digest, Sha256};

/// Computes SHA-256 hash of a raw token string and returns hex-encoded string.
pub fn hash_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    hex::encode(hasher.finalize())
}
