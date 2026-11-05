use axum::{async_trait, extract::FromRequestParts, http::request::Parts};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    constants::codes::{CODE_ACCESS_TOKEN_EXPIRED, CODE_INVALID_ACCESS_TOKEN, CODE_UNAUTHORIZED},
    pkg::errors::AppError,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,   // user id
    pub email: String, // user email
    pub role: String,  // user role
    pub exp: usize,    // expiration timestamp
    pub iat: usize,    // issued at timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    pub id: Uuid,
    pub email: String,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: chrono::DateTime<Utc>,
    pub token_type: String,
}

pub fn generate_token_pair(
    user_id: &Uuid,
    email: &str,
    role: &str,
    secret: &str,
    access_expiry_minutes: i64,
) -> Result<TokenPair, AppError> {
    let now = Utc::now();
    let access_exp = now + Duration::minutes(access_expiry_minutes);

    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        role: role.to_string(),
        exp: access_exp.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    let access_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::internal(format!("Token generation failed: {}", e)))?;

    // Cryptographically random refresh token
    let refresh_token = format!("{}.{}", Uuid::new_v4(), Uuid::new_v4());

    Ok(TokenPair {
        access_token,
        refresh_token,
        expires_at: access_exp,
        token_type: "Bearer".to_string(),
    })
}

pub fn verify_access_token(token: &str, secret: &str) -> Result<Claims, AppError> {
    let mut validation = Validation::default();
    validation.validate_exp = true;

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|err| match err.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
            AppError::unauthorized("Access token has expired", CODE_ACCESS_TOKEN_EXPIRED)
        }
        _ => AppError::unauthorized("Invalid access token", CODE_INVALID_ACCESS_TOKEN),
    })
}

/// Axum extractor for authenticated routes.
#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| {
                AppError::unauthorized("Authorization header missing", CODE_UNAUTHORIZED)
            })?;

        if !auth_header.starts_with("Bearer ") {
            return Err(AppError::unauthorized(
                "Invalid Authorization scheme, must be Bearer",
                CODE_UNAUTHORIZED,
            ));
        }

        let token = &auth_header[7..];

        // Read JWT secret from environment
        let secret = std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| "default_secret_key_change_in_production".to_string());
        let claims = verify_access_token(token, &secret)?;

        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| {
            AppError::unauthorized("Invalid user ID in token", CODE_INVALID_ACCESS_TOKEN)
        })?;

        Ok(AuthenticatedUser {
            id: user_id,
            email: claims.email,
            role: claims.role,
        })
    }
}

/// Extractor for routes that strictly require the `admin` role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminUser(pub AuthenticatedUser);

impl std::ops::Deref for AdminUser {
    type Target = AuthenticatedUser;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AdminUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;
        if user.role != "admin" {
            return Err(AppError::forbidden(
                "Forbidden: Admin privileges required",
                crate::constants::codes::CODE_FORBIDDEN,
            ));
        }
        Ok(AdminUser(user))
    }
}

/// Extractor for public routes that optionally accept an authenticated user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalAuthUser(pub Option<AuthenticatedUser>);

impl std::ops::Deref for OptionalAuthUser {
    type Target = Option<AuthenticatedUser>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for OptionalAuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match parts.headers.get(axum::http::header::AUTHORIZATION) {
            Some(_) => {
                let user = AuthenticatedUser::from_request_parts(parts, state).await?;
                Ok(OptionalAuthUser(Some(user)))
            }
            None => Ok(OptionalAuthUser(None)),
        }
    }
}
