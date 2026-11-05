use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct UserResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate, Clone)]
pub struct CreateUserRequest {
    #[validate(length(
        min = 2,
        max = 100,
        message = "Name must be between 2 and 100 characters"
    ))]
    pub name: String,

    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    #[validate(length(min = 10, max = 20, message = "Phone must be between 10 and 20 digits"))]
    pub phone: Option<String>,

    pub role: Option<String>,

    #[validate(length(min = 8, max = 100, message = "Password must be at least 8 characters"))]
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, Validate, Clone)]
pub struct UpdateUserRequest {
    #[validate(length(
        min = 2,
        max = 100,
        message = "Name must be between 2 and 100 characters"
    ))]
    pub name: Option<String>,

    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,

    #[validate(length(min = 10, max = 20, message = "Phone must be between 10 and 20 digits"))]
    pub phone: Option<String>,

    pub role: Option<String>,

    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize, Validate, Clone)]
pub struct UpdateProfileRequest {
    #[validate(length(
        min = 2,
        max = 100,
        message = "Name must be between 2 and 100 characters"
    ))]
    pub name: Option<String>,

    #[validate(length(min = 10, max = 20, message = "Phone must be between 10 and 20 digits"))]
    pub phone: Option<String>,
}

#[derive(Debug, Deserialize, Validate, Clone, Default)]
pub struct ListUsersQuery {
    #[validate(range(min = 1, message = "Page must be greater than or equal to 1"))]
    pub page: Option<u64>,

    #[validate(range(min = 1, max = 100, message = "Limit must be between 1 and 100"))]
    pub limit: Option<u64>,

    pub search: Option<String>,
    pub role: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize, Validate, Clone)]
pub struct UserPathParams {
    pub id: Uuid,
}
