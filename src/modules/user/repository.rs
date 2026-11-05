use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    modules::user::dto::{ListUsersQuery, UpdateProfileRequest, UpdateUserRequest, UserResponse},
    pkg::errors::AppError,
};

#[derive(Debug, FromRow, Clone)]
pub struct UserRecord {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub password: String,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewUser<'a> {
    pub name: &'a str,
    pub email: &'a str,
    pub phone: Option<&'a str>,
    pub password: &'a str,
    pub role: &'a str,
    pub is_active: bool,
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, pool: &PgPool, id: Uuid) -> Result<Option<UserResponse>, AppError>;
    async fn find_by_email(
        &self,
        pool: &PgPool,
        email: &str,
    ) -> Result<Option<UserRecord>, AppError>;
    async fn exists_by_id(&self, pool: &PgPool, id: Uuid) -> Result<bool, AppError>;
    async fn exists_by_email(&self, pool: &PgPool, email: &str) -> Result<bool, AppError>;
    async fn create(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        new_user: NewUser<'_>,
    ) -> Result<UserResponse, AppError>;
    async fn update(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        req: &UpdateUserRequest,
    ) -> Result<UserResponse, AppError>;
    async fn update_profile(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        req: &UpdateProfileRequest,
    ) -> Result<UserResponse, AppError>;
    async fn delete(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<(), AppError>;
    async fn hard_delete(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<(), AppError>;
    async fn list(
        &self,
        pool: &PgPool,
        query: &ListUsersQuery,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<UserResponse>, i64), AppError>;
}

#[derive(Clone, Default)]
pub struct PostgresUserRepository;

impl PostgresUserRepository {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UserRepository for PostgresUserRepository {
    async fn find_by_id(&self, pool: &PgPool, id: Uuid) -> Result<Option<UserResponse>, AppError> {
        let sql = r#"
            SELECT id, name, email, phone, role, is_active, created_at, updated_at
            FROM users
            WHERE id = $1
        "#;

        let row = sqlx::query_as::<_, UserResponse>(sql)
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(AppError::Database)?;

        Ok(row)
    }

    async fn find_by_email(
        &self,
        pool: &PgPool,
        email: &str,
    ) -> Result<Option<UserRecord>, AppError> {
        let sql = r#"
            SELECT id, name, email, phone, password, role, is_active, created_at, updated_at
            FROM users
            WHERE email = $1
        "#;

        let row = sqlx::query_as::<_, UserRecord>(sql)
            .bind(email)
            .fetch_optional(pool)
            .await
            .map_err(AppError::Database)?;

        Ok(row)
    }

    async fn exists_by_id(&self, pool: &PgPool, id: Uuid) -> Result<bool, AppError> {
        let sql = "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1) as exists";
        let row = sqlx::query(sql)
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(AppError::Database)?;

        Ok(row.try_get("exists").unwrap_or(false))
    }

    async fn exists_by_email(&self, pool: &PgPool, email: &str) -> Result<bool, AppError> {
        let sql = "SELECT EXISTS(SELECT 1 FROM users WHERE email = $1) as exists";
        let row = sqlx::query(sql)
            .bind(email)
            .fetch_one(pool)
            .await
            .map_err(AppError::Database)?;

        Ok(row.try_get("exists").unwrap_or(false))
    }

    async fn create(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        new_user: NewUser<'_>,
    ) -> Result<UserResponse, AppError> {
        let sql = r#"
            INSERT INTO users (name, email, phone, password, role, is_active)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, name, email, phone, role, is_active, created_at, updated_at
        "#;

        let user = sqlx::query_as::<_, UserResponse>(sql)
            .bind(new_user.name)
            .bind(new_user.email)
            .bind(new_user.phone)
            .bind(new_user.password)
            .bind(new_user.role)
            .bind(new_user.is_active)
            .fetch_one(&mut **tx)
            .await
            .map_err(AppError::Database)?;

        Ok(user)
    }

    async fn update(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        req: &UpdateUserRequest,
    ) -> Result<UserResponse, AppError> {
        let sql = r#"
            UPDATE users
            SET name = COALESCE($1, name),
                email = COALESCE($2, email),
                phone = COALESCE($3, phone),
                role = COALESCE($4, role),
                is_active = COALESCE($5, is_active),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $6
            RETURNING id, name, email, phone, role, is_active, created_at, updated_at
        "#;

        let user = sqlx::query_as::<_, UserResponse>(sql)
            .bind(req.name.as_deref())
            .bind(req.email.as_deref())
            .bind(req.phone.as_deref())
            .bind(req.role.as_deref())
            .bind(req.is_active)
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(AppError::user_not_found)?;

        Ok(user)
    }

    async fn update_profile(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        req: &UpdateProfileRequest,
    ) -> Result<UserResponse, AppError> {
        let sql = r#"
            UPDATE users
            SET name = COALESCE($1, name),
                phone = COALESCE($2, phone),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $3
            RETURNING id, name, email, phone, role, is_active, created_at, updated_at
        "#;

        let user = sqlx::query_as::<_, UserResponse>(sql)
            .bind(req.name.as_deref())
            .bind(req.phone.as_deref())
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(AppError::user_not_found)?;

        Ok(user)
    }

    async fn delete(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<(), AppError> {
        let sql = r#"
            UPDATE users
            SET is_active = false, updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
        "#;

        let rows_affected = sqlx::query(sql)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::Database)?
            .rows_affected();

        if rows_affected == 0 {
            return Err(AppError::user_not_found());
        }

        Ok(())
    }

    async fn hard_delete(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<(), AppError> {
        let sql = "DELETE FROM users WHERE id = $1";

        let rows_affected = sqlx::query(sql)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::Database)?
            .rows_affected();

        if rows_affected == 0 {
            return Err(AppError::user_not_found());
        }

        Ok(())
    }

    async fn list(
        &self,
        pool: &PgPool,
        query: &ListUsersQuery,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<UserResponse>, i64), AppError> {
        let search_pattern = query.search.as_ref().map(|s| format!("%{}%", s));

        let sql = r#"
            SELECT id, name, email, phone, role, is_active, created_at, updated_at
            FROM users
            WHERE ($1::TEXT IS NULL OR name ILIKE $1 OR email ILIKE $1)
              AND ($2::TEXT IS NULL OR role = $2)
              AND ($3::BOOLEAN IS NULL OR is_active = $3)
            ORDER BY created_at DESC
            LIMIT $4 OFFSET $5
        "#;

        let users = sqlx::query_as::<_, UserResponse>(sql)
            .bind(search_pattern.as_deref())
            .bind(query.role.as_deref())
            .bind(query.is_active)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(AppError::Database)?;

        let count_sql = r#"
            SELECT COUNT(*) as count
            FROM users
            WHERE ($1::TEXT IS NULL OR name ILIKE $1 OR email ILIKE $1)
              AND ($2::TEXT IS NULL OR role = $2)
              AND ($3::BOOLEAN IS NULL OR is_active = $3)
        "#;

        let count_row = sqlx::query(count_sql)
            .bind(search_pattern.as_deref())
            .bind(query.role.as_deref())
            .bind(query.is_active)
            .fetch_one(pool)
            .await
            .map_err(AppError::Database)?;

        let total: i64 = count_row.try_get("count").unwrap_or(0);

        Ok((users, total))
    }
}
