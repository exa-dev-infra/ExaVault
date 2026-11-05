use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::Serialize;
use thiserror::Error;

use crate::constants::codes::*;

#[derive(Debug, Serialize, Clone)]
pub struct ErrorResponse {
    pub message: String,
    pub success: bool,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<serde_json::Value>,
    pub timestamp: String,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Validation failed: {message}")]
    Validation {
        message: String,
        details: Option<serde_json::Value>,
    },

    #[error("Bad request: {0}")]
    BadRequest(String, String), // (message, code)

    #[error("Unauthorized: {0}")]
    Unauthorized(String, String), // (message, code)

    #[error("Forbidden: {0}")]
    Forbidden(String, String), // (message, code)

    #[error("Not found: {0}")]
    NotFound(String, String), // (message, code)

    #[error("Conflict: {0}")]
    Conflict(String, String), // (message, code)

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn bad_request(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self::BadRequest(message.into(), code.into())
    }

    pub fn unauthorized(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self::Unauthorized(message.into(), code.into())
    }

    pub fn forbidden(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self::Forbidden(message.into(), code.into())
    }

    pub fn not_found(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self::NotFound(message.into(), code.into())
    }

    pub fn conflict(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self::Conflict(message.into(), code.into())
    }

    pub fn validation(message: impl Into<String>, details: Option<serde_json::Value>) -> Self {
        Self::Validation {
            message: message.into(),
            details,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    // Specific domain errors
    pub fn user_not_found() -> Self {
        Self::NotFound(
            "User not found".to_string(),
            CODE_USER_NOT_FOUND.to_string(),
        )
    }

    pub fn email_already_exists() -> Self {
        Self::Conflict(
            "Email already registered".to_string(),
            CODE_EMAIL_ALREADY_EXISTS.to_string(),
        )
    }

    pub fn invalid_credentials() -> Self {
        Self::Unauthorized(
            "Invalid email or password".to_string(),
            CODE_INVALID_CREDENTIALS.to_string(),
        )
    }

    pub fn invalid_refresh_token() -> Self {
        Self::Unauthorized(
            "Invalid or expired refresh token".to_string(),
            CODE_INVALID_REFRESH_TOKEN.to_string(),
        )
    }

    pub fn refresh_token_required() -> Self {
        Self::Unauthorized(
            "Refresh token is required".to_string(),
            CODE_REFRESH_TOKEN_REQUIRED.to_string(),
        )
    }
}

/// Parses raw database errors (e.g. PostgreSQL SQLSTATE codes) into user-friendly semantic responses.
pub fn parse_database_error(err: &sqlx::Error) -> (StatusCode, String, String) {
    if let sqlx::Error::Database(db_err) = err {
        if let Some(code) = db_err.code() {
            match code.as_ref() {
                "23505" => {
                    // Unique constraint violation (SQLSTATE 23505)
                    let constraint = db_err.constraint().unwrap_or("");

                    // 1. Check explicit developer-defined mapping in constants::database
                    if let Some((field, sem_code)) =
                        crate::constants::database::lookup_unique_constraint(constraint)
                    {
                        return (
                            StatusCode::CONFLICT,
                            sem_code.to_string(),
                            format!("The {} already exists", field),
                        );
                    }

                    // 2. Auto-deduce field name from PostgreSQL standard naming: <table>_<column>_key
                    let parts: Vec<&str> = constraint.split('_').collect();
                    if parts.len() >= 3 && parts.last() == Some(&"key") {
                        let field = parts[parts.len() - 2];
                        return (
                            StatusCode::CONFLICT,
                            CODE_CONFLICT.to_string(),
                            format!("The {} already exists", field),
                        );
                    }

                    return (
                        StatusCode::CONFLICT,
                        CODE_CONFLICT.to_string(),
                        "A record with this unique identifier already exists".to_string(),
                    );
                }
                "23503" => {
                    // Foreign key violation (SQLSTATE 23503)
                    let constraint = db_err.constraint().unwrap_or("");

                    // 1. Check explicit developer-defined mapping in constants::database
                    if let Some(entity) =
                        crate::constants::database::lookup_foreign_key_constraint(constraint)
                    {
                        return (
                            StatusCode::BAD_REQUEST,
                            CODE_BAD_REQUEST.to_string(),
                            format!("Invalid {} reference: record does not exist", entity),
                        );
                    }

                    // 2. Auto-deduce from PostgreSQL standard naming: <table>_<column>_fkey
                    let parts: Vec<&str> = constraint.split('_').collect();
                    if parts.len() >= 3 && parts.last() == Some(&"fkey") {
                        let field = parts[parts.len() - 2];
                        return (
                            StatusCode::BAD_REQUEST,
                            CODE_BAD_REQUEST.to_string(),
                            format!("Invalid {} reference: record does not exist", field),
                        );
                    }

                    return (
                        StatusCode::BAD_REQUEST,
                        CODE_BAD_REQUEST.to_string(),
                        "Referenced resource does not exist".to_string(),
                    );
                }
                "23502" => {
                    // Not-null constraint violation (SQLSTATE 23502)
                    return (
                        StatusCode::BAD_REQUEST,
                        CODE_BAD_REQUEST.to_string(),
                        "A required database field is missing".to_string(),
                    );
                }
                "23514" => {
                    // Check constraint violation (SQLSTATE 23514)
                    return (
                        StatusCode::BAD_REQUEST,
                        CODE_BAD_REQUEST.to_string(),
                        "Database check constraint violation".to_string(),
                    );
                }
                _ => {}
            }
        }
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        CODE_DATABASE_ERROR.to_string(),
        "Internal database error occurred".to_string(),
    )
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, errors) = match self {
            Self::Validation { message, details } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                CODE_VALIDATION_ERROR.to_string(),
                message,
                details,
            ),
            Self::BadRequest(msg, code) => (StatusCode::BAD_REQUEST, code, msg, None),
            Self::Unauthorized(msg, code) => (StatusCode::UNAUTHORIZED, code, msg, None),
            Self::Forbidden(msg, code) => (StatusCode::FORBIDDEN, code, msg, None),
            Self::NotFound(msg, code) => (StatusCode::NOT_FOUND, code, msg, None),
            Self::Conflict(msg, code) => (StatusCode::CONFLICT, code, msg, None),
            Self::Database(err) => {
                let (status, code, message) = parse_database_error(&err);
                if status == StatusCode::INTERNAL_SERVER_ERROR {
                    tracing::error!(error = %err, "Database error encountered");
                } else {
                    tracing::warn!(error = %err, code = %code, "Handled database constraint violation");
                }
                (status, code, message, None)
            }
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "Internal error encountered");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    CODE_INTERNAL_ERROR.to_string(),
                    "Internal server error occurred".to_string(),
                    None,
                )
            }
        };

        let body = Json(ErrorResponse {
            message,
            success: false,
            code,
            errors,
            timestamp: Utc::now().to_rfc3339(),
        });

        (status, body).into_response()
    }
}
