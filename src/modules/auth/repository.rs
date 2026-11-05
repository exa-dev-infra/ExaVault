use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{modules::auth::dto::SessionResponse, pkg::errors::AppError};

#[derive(Debug, FromRow, Clone)]
pub struct RefreshTokenRecord {
    pub id: i32,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[async_trait]
pub trait AuthRepository: Send + Sync {
    async fn create_refresh_token(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
    ) -> Result<(), AppError>;

    async fn find_token_by_hash(
        &self,
        pool: &PgPool,
        token_hash: &str,
    ) -> Result<Option<RefreshTokenRecord>, AppError>;

    async fn revoke_refresh_token(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        token_hash: &str,
    ) -> Result<(), AppError>;

    async fn revoke_all_user_tokens(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
    ) -> Result<(), AppError>;

    async fn list_active_sessions(
        &self,
        pool: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<SessionResponse>, AppError>;

    async fn update_user_password(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        password_hash: &str,
    ) -> Result<(), AppError>;
}

#[derive(Clone, Default)]
pub struct PostgresAuthRepository;

impl PostgresAuthRepository {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AuthRepository for PostgresAuthRepository {
    async fn create_refresh_token(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
    ) -> Result<(), AppError> {
        let sql = r#"
            INSERT INTO refresh_tokens (user_id, token_hash, expires_at, user_agent, ip_address)
            VALUES ($1, $2, $3, $4, $5)
        "#;

        sqlx::query(sql)
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .bind(user_agent)
            .bind(ip_address)
            .execute(&mut **tx)
            .await
            .map_err(AppError::Database)?;

        Ok(())
    }

    async fn find_token_by_hash(
        &self,
        pool: &PgPool,
        token_hash: &str,
    ) -> Result<Option<RefreshTokenRecord>, AppError> {
        let sql = r#"
            SELECT id, user_id, token_hash, expires_at, revoked_at, user_agent, ip_address, created_at, updated_at
            FROM refresh_tokens
            WHERE token_hash = $1
        "#;

        let token = sqlx::query_as::<_, RefreshTokenRecord>(sql)
            .bind(token_hash)
            .fetch_optional(pool)
            .await
            .map_err(AppError::Database)?;

        Ok(token)
    }

    async fn revoke_refresh_token(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        token_hash: &str,
    ) -> Result<(), AppError> {
        let sql = r#"
            UPDATE refresh_tokens
            SET revoked_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
            WHERE token_hash = $1 AND revoked_at IS NULL
        "#;

        sqlx::query(sql)
            .bind(token_hash)
            .execute(&mut **tx)
            .await
            .map_err(AppError::Database)?;

        Ok(())
    }

    async fn revoke_all_user_tokens(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let sql = r#"
            UPDATE refresh_tokens
            SET revoked_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
            WHERE user_id = $1 AND revoked_at IS NULL
        "#;

        sqlx::query(sql)
            .bind(user_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::Database)?;

        Ok(())
    }

    async fn list_active_sessions(
        &self,
        pool: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<SessionResponse>, AppError> {
        let sql = r#"
            SELECT id, user_agent, ip_address, created_at, expires_at
            FROM refresh_tokens
            WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > CURRENT_TIMESTAMP
            ORDER BY created_at DESC
        "#;

        let sessions = sqlx::query_as::<
            _,
            (
                i32,
                Option<String>,
                Option<String>,
                DateTime<Utc>,
                DateTime<Utc>,
            ),
        >(sql)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::Database)?
        .into_iter()
        .map(|(id, ua, ip, created, expires)| SessionResponse {
            id,
            user_agent: ua,
            ip_address: ip,
            created_at: created,
            expires_at: expires,
        })
        .collect();

        Ok(sessions)
    }

    async fn update_user_password(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        password_hash: &str,
    ) -> Result<(), AppError> {
        let sql = r#"
            UPDATE users
            SET password = $1, updated_at = CURRENT_TIMESTAMP
            WHERE id = $2
        "#;

        sqlx::query(sql)
            .bind(password_hash)
            .bind(user_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::Database)?;

        Ok(())
    }
}
