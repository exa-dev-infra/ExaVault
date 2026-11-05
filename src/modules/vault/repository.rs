use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    constants::codes::*,
    modules::vault::dto::*,
    pkg::errors::AppError,
};

#[derive(Clone)]
pub struct VaultRepository {
    pool: PgPool,
}

impl VaultRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn list_projects(&self) -> Result<Vec<ProjectWithEnvironments>, AppError> {
        let projects: Vec<Project> = sqlx::query_as(
            "SELECT id, name, slug, description, group_name, created_at FROM projects ORDER BY COALESCE(group_name, '') ASC, name ASC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let mut result = Vec::with_capacity(projects.len());
        for project in projects {
            let envs: Vec<Environment> = sqlx::query_as(
                "SELECT id, project_id, name, slug, created_at FROM environments WHERE project_id = $1 ORDER BY created_at ASC"
            )
            .bind(project.id)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::Database)?;

            result.push(ProjectWithEnvironments {
                project,
                environments: envs,
            });
        }

        Ok(result)
    }

    pub async fn create_project(
        &self,
        name: &str,
        slug: &str,
        description: Option<&str>,
        group_name: Option<&str>,
    ) -> Result<ProjectWithEnvironments, AppError> {
        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        let project: Project = sqlx::query_as(
            "INSERT INTO projects (name, slug, description, group_name) VALUES ($1, $2, $3, $4) RETURNING id, name, slug, description, group_name, created_at"
        )
        .bind(name)
        .bind(slug)
        .bind(description)
        .bind(group_name)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::Database)?;

        let default_envs = [
            ("Development", "dev"),
            ("Staging", "staging"),
            ("Production", "prod"),
        ];

        let mut envs = Vec::with_capacity(3);
        for (env_name, env_slug) in default_envs {
            let env: Environment = sqlx::query_as(
                "INSERT INTO environments (project_id, name, slug) VALUES ($1, $2, $3) RETURNING id, project_id, name, slug, created_at"
            )
            .bind(project.id)
            .bind(env_name)
            .bind(env_slug)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::Database)?;
            envs.push(env);
        }

        tx.commit().await.map_err(AppError::Database)?;

        Ok(ProjectWithEnvironments {
            project,
            environments: envs,
        })
    }

    pub async fn delete_project(&self, id: Uuid) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Project not found", CODE_PROJECT_NOT_FOUND));
        }

        Ok(())
    }

    pub async fn get_environment(&self, id: Uuid) -> Result<Environment, AppError> {
        sqlx::query_as(
            "SELECT id, project_id, name, slug, created_at FROM environments WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::not_found("Environment not found", CODE_ENVIRONMENT_NOT_FOUND))
    }

    pub async fn list_secrets(&self, env_id: Uuid) -> Result<Vec<Secret>, AppError> {
        // Verify environment exists
        let _ = self.get_environment(env_id).await?;

        let secrets: Vec<Secret> = sqlx::query_as(
            "SELECT id, environment_id, key, encrypted_value, iv, auth_tag, comment, updated_at 
             FROM secrets 
             WHERE environment_id = $1 
             ORDER BY key ASC"
        )
        .bind(env_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(secrets)
    }

    pub async fn bulk_upsert_secrets(
        &self,
        env_id: Uuid,
        items: Vec<(String, String, String, String, Option<String>)>,
    ) -> Result<(), AppError> {
        let _ = self.get_environment(env_id).await?;

        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        for (key, encrypted_val, iv, auth_tag, comment) in items {
            sqlx::query(
                "INSERT INTO secrets (environment_id, key, encrypted_value, iv, auth_tag, comment, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW())
                 ON CONFLICT (environment_id, key) DO UPDATE 
                 SET encrypted_value = EXCLUDED.encrypted_value,
                     iv = EXCLUDED.iv,
                     auth_tag = EXCLUDED.auth_tag,
                     comment = EXCLUDED.comment,
                     updated_at = NOW()"
            )
            .bind(env_id)
            .bind(key)
            .bind(encrypted_val)
            .bind(iv)
            .bind(auth_tag)
            .bind(comment)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;
        }

        tx.commit().await.map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn delete_secret(&self, id: Uuid) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM secrets WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Secret not found", CODE_SECRET_NOT_FOUND));
        }

        Ok(())
    }

    pub async fn list_tokens(&self, env_id: Uuid) -> Result<Vec<ServiceTokenListItem>, AppError> {
        let _ = self.get_environment(env_id).await?;

        let rows: Vec<ServiceToken> = sqlx::query_as(
            "SELECT id, environment_id, name, token_prefix, token_hash, last_used_at, expires_at, created_at 
             FROM service_tokens 
             WHERE environment_id = $1 
             ORDER BY created_at DESC"
        )
        .bind(env_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let list = rows
            .into_iter()
            .map(|t| ServiceTokenListItem {
                id: t.id,
                environment_id: t.environment_id,
                name: t.name,
                token_prefix: t.token_prefix,
                last_used_at: t.last_used_at,
                expires_at: t.expires_at,
                created_at: t.created_at,
            })
            .collect();

        Ok(list)
    }

    pub async fn create_token(
        &self,
        env_id: Uuid,
        name: &str,
        token_prefix: &str,
        token_hash: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<ServiceToken, AppError> {
        let _ = self.get_environment(env_id).await?;

        let record: ServiceToken = sqlx::query_as(
            "INSERT INTO service_tokens (environment_id, name, token_prefix, token_hash, expires_at)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, environment_id, name, token_prefix, token_hash, last_used_at, expires_at, created_at"
        )
        .bind(env_id)
        .bind(name)
        .bind(token_prefix)
        .bind(token_hash)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(record)
    }

    pub async fn delete_token(&self, id: Uuid) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM service_tokens WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Service token not found", CODE_TOKEN_NOT_FOUND));
        }

        Ok(())
    }

    pub async fn find_token_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<(ServiceToken, Environment), AppError> {
        let token: ServiceToken = sqlx::query_as(
            "SELECT id, environment_id, name, token_prefix, token_hash, last_used_at, expires_at, created_at
             FROM service_tokens
             WHERE token_hash = $1"
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::unauthorized("Invalid service token", CODE_UNAUTHORIZED))?;

        let env = self.get_environment(token.environment_id).await?;
        Ok((token, env))
    }

    pub async fn touch_token_last_used(&self, id: Uuid) -> Result<(), AppError> {
        sqlx::query("UPDATE service_tokens SET last_used_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        Ok(())
    }
}

