use exavault::pkg::auth::{
    jwt::{generate_token_pair, verify_access_token},
    password::{hash_password, verify_password},
    token_hash::hash_token,
};
use uuid::Uuid;

#[test]
fn test_password_hashing_and_verification() {
    let password = "SuperSecretPassword2026!";
    let hashed = hash_password(password).expect("hashing should succeed");

    assert_ne!(password, hashed);
    assert!(verify_password(password, &hashed).expect("verification should succeed"));
    assert!(!verify_password("WrongPassword123", &hashed).expect("verification should succeed"));
}

#[test]
fn test_jwt_token_generation_and_validation() {
    let user_id = Uuid::new_v4();
    let email = "test@example.com";
    let role = "admin";
    let secret = "test-jwt-secret-key-12345678901234567890";

    let token_pair = generate_token_pair(&user_id, email, role, secret, 15)
        .expect("token generation should succeed");

    assert!(!token_pair.access_token.is_empty());
    assert!(!token_pair.refresh_token.is_empty());
    assert_eq!(token_pair.token_type, "Bearer");

    let claims = verify_access_token(&token_pair.access_token, secret)
        .expect("token verification should succeed");

    assert_eq!(claims.sub, user_id.to_string());
    assert_eq!(claims.email, email);
    assert_eq!(claims.role, role);

    // Corrupted secret should fail
    let invalid_claims = verify_access_token(&token_pair.access_token, "wrong-secret-key");
    assert!(invalid_claims.is_err());
}

#[test]
fn test_sha256_token_hashing() {
    let token = "raw-refresh-token-abc-123";
    let digest1 = hash_token(token);
    let digest2 = hash_token(token);

    assert_eq!(digest1, digest2);
    assert_eq!(digest1.len(), 64); // SHA-256 produces 64 hex characters
    assert_ne!(token, digest1);
}
