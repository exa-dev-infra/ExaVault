use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::pkg::auth::jwt::TokenPair;

#[derive(Debug, Deserialize, Validate, Clone)]
pub struct RegisterRequest {
    #[validate(length(
        min = 2,
        max = 100,
        message = "Name must be between 2 and 100 characters"
    ))]
    pub name: String,

    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    #[validate(length(min = 8, max = 100, message = "Password must be at least 8 characters"))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate, Clone)]
pub struct LoginRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    #[validate(length(min = 1, message = "Password is required"))]
    pub password: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct RefreshTokenRequest {
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize, Validate, Clone)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 1, message = "Current password is required"))]
    pub current_password: String,

    #[validate(length(
        min = 8,
        max = 100,
        message = "New password must be at least 8 characters"
    ))]
    pub new_password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionResponse {
    pub id: i32,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserPayload {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthResponse {
    pub user: UserPayload,
    pub tokens: TokenPair,
}
