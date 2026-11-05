use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    modules::{
        auth::repository::{AuthRepository, PostgresAuthRepository},
        user::{
            dto::{
                CreateUserRequest, ListUsersQuery, UpdateProfileRequest, UpdateUserRequest,
                UserResponse,
            },
            repository::{NewUser, PostgresUserRepository, UserRepository},
        },
    },
    pkg::{auth::password::hash_password, errors::AppError},
};

pub struct UserService {
    repo: Arc<dyn UserRepository>,
    auth_repo: Arc<dyn AuthRepository>,
    pool: PgPool,
}

impl UserService {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        auth_repo: Arc<dyn AuthRepository>,
        pool: PgPool,
    ) -> Self {
        Self {
            repo,
            auth_repo,
            pool,
        }
    }

    pub fn with_default_repo(pool: PgPool) -> Self {
        Self {
            repo: Arc::new(PostgresUserRepository::new()),
            auth_repo: Arc::new(PostgresAuthRepository::new()),
            pool,
        }
    }

    /// Retrieves an active user profile by their ID.
    pub async fn get_profile(&self, user_id: Uuid) -> Result<UserResponse, AppError> {
        self.repo
            .find_by_id(&self.pool, user_id)
            .await?
            .ok_or_else(AppError::user_not_found)
    }

    /// Updates current user profile (name, phone) within a database transaction.
    pub async fn update_profile(
        &self,
        user_id: Uuid,
        req: UpdateProfileRequest,
    ) -> Result<UserResponse, AppError> {
        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        let updated_user = self.repo.update_profile(&mut tx, user_id, &req).await?;

        tx.commit().await.map_err(AppError::Database)?;

        Ok(updated_user)
    }

    /// Retrieves a single user by ID for administrative views.
    pub async fn get_user_by_id(&self, id: Uuid) -> Result<UserResponse, AppError> {
        self.repo
            .find_by_id(&self.pool, id)
            .await?
            .ok_or_else(AppError::user_not_found)
    }

    /// Admin endpoint to create a user within a transaction.
    pub async fn create_user(&self, req: CreateUserRequest) -> Result<UserResponse, AppError> {
        if self.repo.exists_by_email(&self.pool, &req.email).await? {
            return Err(AppError::email_already_exists());
        }

        let raw_password = req
            .password
            .unwrap_or_else(|| "TemporaryPassword123!".to_string());
        let hashed_password = hash_password(&raw_password)?;

        let role = req.role.unwrap_or_else(|| "user".to_string());

        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        let user = self
            .repo
            .create(
                &mut tx,
                NewUser {
                    name: &req.name,
                    email: &req.email,
                    phone: req.phone.as_deref(),
                    password: &hashed_password,
                    role: &role,
                    is_active: true,
                },
            )
            .await?;

        tx.commit().await.map_err(AppError::Database)?;

        Ok(user)
    }

    /// Admin endpoint to update any user field within a transaction.
    pub async fn update_user(
        &self,
        id: Uuid,
        req: UpdateUserRequest,
    ) -> Result<UserResponse, AppError> {
        if !self.repo.exists_by_id(&self.pool, id).await? {
            return Err(AppError::user_not_found());
        }

        if let Some(new_email) = &req.email {
            if let Some(existing) = self.repo.find_by_email(&self.pool, new_email).await? {
                if existing.id != id {
                    return Err(AppError::email_already_exists());
                }
            }
        }

        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        let updated_user = self.repo.update(&mut tx, id, &req).await?;

        // If deactivated, revoke all their active sessions
        if let Some(false) = req.is_active {
            self.auth_repo.revoke_all_user_tokens(&mut tx, id).await?;
        }

        tx.commit().await.map_err(AppError::Database)?;

        Ok(updated_user)
    }

    /// Soft delete user by ID (`is_active = false`) and revoke all sessions.
    pub async fn delete_user(&self, id: Uuid) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        self.repo.delete(&mut tx, id).await?;
        self.auth_repo.revoke_all_user_tokens(&mut tx, id).await?;

        tx.commit().await.map_err(AppError::Database)?;

        Ok(())
    }

    /// Hard delete user permanently from the database.
    pub async fn hard_delete_user(&self, id: Uuid) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        self.repo.hard_delete(&mut tx, id).await?;

        tx.commit().await.map_err(AppError::Database)?;

        Ok(())
    }

    /// Paginated, filterable list of users.
    pub async fn list_users(
        &self,
        query: ListUsersQuery,
    ) -> Result<(Vec<UserResponse>, u64, u64, u64), AppError> {
        let page = query.page.unwrap_or(1).max(1);
        let limit = query.limit.unwrap_or(10).clamp(1, 100);
        let offset = ((page - 1) * limit) as i64;

        let (users, total) = self
            .repo
            .list(&self.pool, &query, limit as i64, offset)
            .await?;

        Ok((users, page, limit, total as u64))
    }
}
