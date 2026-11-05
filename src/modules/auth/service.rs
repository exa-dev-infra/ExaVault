use chrono::{Duration, Utc};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    constants::codes::{
        CODE_CURRENT_PASSWORD_INVALID, CODE_INVALID_REFRESH_TOKEN, CODE_REFRESH_TOKEN_EXPIRED,
    },
    modules::{
        auth::{
            dto::{
                AuthResponse, ChangePasswordRequest, LoginRequest, RegisterRequest,
                SessionResponse, UserPayload,
            },
            repository::{AuthRepository, PostgresAuthRepository},
        },
        user::repository::{NewUser, PostgresUserRepository, UserRepository},
    },
    pkg::{
        auth::{
            jwt::generate_token_pair,
            password::{hash_password, verify_password},
            token_hash::hash_token,
        },
        errors::AppError,
    },
};

pub struct AuthService {
    auth_repo: Arc<dyn AuthRepository>,
    user_repo: Arc<dyn UserRepository>,
    pool: PgPool,
    config: AppConfig,
}

impl AuthService {
    pub fn new(
        auth_repo: Arc<dyn AuthRepository>,
        user_repo: Arc<dyn UserRepository>,
        pool: PgPool,
        config: AppConfig,
    ) -> Self {
        Self {
            auth_repo,
            user_repo,
            pool,
            config,
        }
    }

    pub fn with_default_repos(pool: PgPool, config: AppConfig) -> Self {
        Self {
            auth_repo: Arc::new(PostgresAuthRepository::new()),
            user_repo: Arc::new(PostgresUserRepository::new()),
            pool,
            config,
        }
    }

    pub async fn register(
        &self,
        req: RegisterRequest,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
    ) -> Result<AuthResponse, AppError> {
        // 1. Check existing email
        if self
            .user_repo
            .find_by_email(&self.pool, &req.email)
            .await?
            .is_some()
        {
            return Err(AppError::email_already_exists());
        }

        // 2. Hash password with Argon2id
        let hashed_password = hash_password(&req.password)?;

        // 3. Database transaction for atomic creation
        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        let created_user = self
            .user_repo
            .create(
                &mut tx,
                NewUser {
                    name: &req.name,
                    email: &req.email,
                    phone: None,
                    password: &hashed_password,
                    role: "user",
                    is_active: true,
                },
            )
            .await?;

        // 4. Generate token pair
        let tokens = generate_token_pair(
            &created_user.id,
            &created_user.email,
            &created_user.role,
            &self.config.jwt_secret,
            self.config.access_token_expiry_minutes,
        )?;

        // 5. Store SHA-256 hash of refresh token in database (never plain text!)
        let token_digest = hash_token(&tokens.refresh_token);
        let refresh_expiry = Utc::now() + Duration::days(self.config.refresh_token_expiry_days);

        self.auth_repo
            .create_refresh_token(
                &mut tx,
                created_user.id,
                &token_digest,
                refresh_expiry,
                user_agent,
                ip_address,
            )
            .await?;

        tx.commit().await.map_err(AppError::Database)?;

        Ok(AuthResponse {
            user: UserPayload {
                id: created_user.id,
                name: created_user.name,
                email: created_user.email,
                phone: created_user.phone,
                role: created_user.role,
                is_active: created_user.is_active,
                created_at: created_user.created_at,
            },
            tokens,
        })
    }

    pub async fn login(
        &self,
        req: LoginRequest,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
    ) -> Result<AuthResponse, AppError> {
        let user_record = self
            .user_repo
            .find_by_email(&self.pool, &req.email)
            .await?
            .ok_or_else(AppError::invalid_credentials)?;

        let is_valid = verify_password(&req.password, &user_record.password)?;
        if !is_valid {
            return Err(AppError::invalid_credentials());
        }

        let tokens = generate_token_pair(
            &user_record.id,
            &user_record.email,
            &user_record.role,
            &self.config.jwt_secret,
            self.config.access_token_expiry_minutes,
        )?;

        let token_digest = hash_token(&tokens.refresh_token);
        let refresh_expiry = Utc::now() + Duration::days(self.config.refresh_token_expiry_days);

        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        self.auth_repo
            .create_refresh_token(
                &mut tx,
                user_record.id,
                &token_digest,
                refresh_expiry,
                user_agent,
                ip_address,
            )
            .await?;

        tx.commit().await.map_err(AppError::Database)?;

        Ok(AuthResponse {
            user: UserPayload {
                id: user_record.id,
                name: user_record.name,
                email: user_record.email,
                phone: user_record.phone,
                role: user_record.role,
                is_active: user_record.is_active,
                created_at: user_record.created_at,
            },
            tokens,
        })
    }

    /// Refresh token rotation with automatic token reuse & theft detection.
    pub async fn refresh(
        &self,
        raw_refresh_token: &str,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
    ) -> Result<AuthResponse, AppError> {
        let token_digest = hash_token(raw_refresh_token);

        let token_record = self
            .auth_repo
            .find_token_by_hash(&self.pool, &token_digest)
            .await?
            .ok_or_else(AppError::invalid_refresh_token)?;

        // 🚨 TOKEN REUSE DETECTION (Security Defense)
        // If an already-revoked refresh token is sent, someone may have stolen it!
        // Immediately revoke ALL active sessions of this user to protect the account.
        if token_record.revoked_at.is_some() {
            warn!(
                user_id = %token_record.user_id,
                token_id = token_record.id,
                "SECURITY ALERT: Revoked refresh token reuse attempt detected! Revoking all sessions for user."
            );

            let mut tx = self.pool.begin().await.map_err(AppError::Database)?;
            self.auth_repo
                .revoke_all_user_tokens(&mut tx, token_record.user_id)
                .await?;
            tx.commit().await.map_err(AppError::Database)?;

            return Err(AppError::unauthorized(
                "Suspicious activity detected, token reused. All sessions revoked. Please log in again.",
                CODE_INVALID_REFRESH_TOKEN,
            ));
        }

        // Check if token has expired
        if token_record.expires_at < Utc::now() {
            return Err(AppError::unauthorized(
                "Refresh token has expired",
                CODE_REFRESH_TOKEN_EXPIRED,
            ));
        }

        let user = self
            .user_repo
            .find_by_id(&self.pool, token_record.user_id)
            .await?
            .ok_or_else(AppError::user_not_found)?;

        let new_tokens = generate_token_pair(
            &user.id,
            &user.email,
            &user.role,
            &self.config.jwt_secret,
            self.config.access_token_expiry_minutes,
        )?;

        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        // Revoke the old token (Single-use rotation guarantee)
        self.auth_repo
            .revoke_refresh_token(&mut tx, &token_digest)
            .await?;

        // Save new hashed refresh token
        let new_token_digest = hash_token(&new_tokens.refresh_token);
        let refresh_expiry = Utc::now() + Duration::days(self.config.refresh_token_expiry_days);

        self.auth_repo
            .create_refresh_token(
                &mut tx,
                user.id,
                &new_token_digest,
                refresh_expiry,
                user_agent,
                ip_address,
            )
            .await?;

        tx.commit().await.map_err(AppError::Database)?;

        Ok(AuthResponse {
            user: UserPayload {
                id: user.id,
                name: user.name,
                email: user.email,
                phone: user.phone,
                role: user.role,
                is_active: user.is_active,
                created_at: user.created_at,
            },
            tokens: new_tokens,
        })
    }

    pub async fn logout(&self, raw_refresh_token: &str) -> Result<(), AppError> {
        let token_digest = hash_token(raw_refresh_token);
        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;
        self.auth_repo
            .revoke_refresh_token(&mut tx, &token_digest)
            .await?;
        tx.commit().await.map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn logout_all(&self, user_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;
        self.auth_repo
            .revoke_all_user_tokens(&mut tx, user_id)
            .await?;
        tx.commit().await.map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn get_sessions(&self, user_id: Uuid) -> Result<Vec<SessionResponse>, AppError> {
        self.auth_repo
            .list_active_sessions(&self.pool, user_id)
            .await
    }

    pub async fn change_password(
        &self,
        user_id: Uuid,
        req: ChangePasswordRequest,
    ) -> Result<(), AppError> {
        let user = self
            .user_repo
            .find_by_id(&self.pool, user_id)
            .await?
            .ok_or_else(AppError::user_not_found)?;

        let user_record = self
            .user_repo
            .find_by_email(&self.pool, &user.email)
            .await?
            .ok_or_else(AppError::user_not_found)?;

        let is_valid = verify_password(&req.current_password, &user_record.password)?;
        if !is_valid {
            return Err(AppError::bad_request(
                "Current password does not match",
                CODE_CURRENT_PASSWORD_INVALID,
            ));
        }

        let new_hash = hash_password(&req.new_password)?;

        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        // Update password
        self.auth_repo
            .update_user_password(&mut tx, user_id, &new_hash)
            .await?;

        // Revoke all existing sessions so old tokens become invalid
        self.auth_repo
            .revoke_all_user_tokens(&mut tx, user_id)
            .await?;

        tx.commit().await.map_err(AppError::Database)?;

        Ok(())
    }

    pub async fn get_current_user(&self, user_id: Uuid) -> Result<UserPayload, AppError> {
        let user = self
            .user_repo
            .find_by_id(&self.pool, user_id)
            .await?
            .ok_or_else(AppError::user_not_found)?;

        Ok(UserPayload {
            id: user.id,
            name: user.name,
            email: user.email,
            phone: user.phone,
            role: user.role,
            is_active: user.is_active,
            created_at: user.created_at,
        })
    }
}
