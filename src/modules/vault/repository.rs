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
            "SELECT p.id, p.name, p.slug, p.description, p.group_id, g.name AS group_name, p.created_at
             FROM projects p
             LEFT JOIN project_groups g ON g.id = p.group_id
             ORDER BY COALESCE(g.name, '') ASC, p.name ASC"
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

    pub async fn get_project(&self, id: Uuid) -> Result<Project, AppError> {
        let project: Project = sqlx::query_as(
            "SELECT p.id, p.name, p.slug, p.description, p.group_id, g.name AS group_name, p.created_at
             FROM projects p
             LEFT JOIN project_groups g ON g.id = p.group_id
             WHERE p.id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::not_found("Project not found", CODE_PROJECT_NOT_FOUND))?;

        Ok(project)
    }

    pub async fn create_project(
        &self,
        name: &str,
        slug: &str,
        description: Option<&str>,
        group_id: Option<Uuid>,
    ) -> Result<ProjectWithEnvironments, AppError> {
        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        let project: Project = sqlx::query_as(
            "INSERT INTO projects (name, slug, description, group_id)
             VALUES ($1, $2, $3, $4)
             RETURNING id, name, slug, description, group_id,
                       (SELECT name FROM project_groups WHERE id = $4) AS group_name,
                       created_at"
        )
        .bind(name)
        .bind(slug)
        .bind(description)
        .bind(group_id)
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

        let mut keys_to_keep = Vec::with_capacity(items.len());
        for (key, encrypted_val, iv, auth_tag, comment) in items {
            keys_to_keep.push(key.clone());
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

        if !keys_to_keep.is_empty() {
            sqlx::query(
                "DELETE FROM secrets WHERE environment_id = $1 AND NOT (key = ANY($2))"
            )
            .bind(env_id)
            .bind(&keys_to_keep)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;
        } else {
            sqlx::query("DELETE FROM secrets WHERE environment_id = $1")
                .bind(env_id)
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

    // -----------------------------------------------------------------------
    // RBAC & Project Member Queries
    // -----------------------------------------------------------------------

    pub async fn list_projects_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ProjectWithEnvironments>, AppError> {
        let projects: Vec<Project> = sqlx::query_as(
            "SELECT p.id, p.name, p.slug, p.description, p.group_id, g.name AS group_name, p.created_at
             FROM projects p
             LEFT JOIN project_groups g ON g.id = p.group_id
             INNER JOIN project_members pm ON pm.project_id = p.id
             WHERE pm.user_id = $1
             ORDER BY COALESCE(g.name, '') ASC, p.name ASC"
        )
        .bind(user_id)
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

    pub async fn get_user_project_role(
        &self,
        user_id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<String>, AppError> {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM project_members WHERE user_id = $1 AND project_id = $2"
        )
        .bind(user_id)
        .bind(project_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(role)
    }

    pub async fn get_project_id_by_env_id(&self, env_id: Uuid) -> Result<Uuid, AppError> {
        let project_id: Uuid = sqlx::query_scalar(
            "SELECT project_id FROM environments WHERE id = $1"
        )
        .bind(env_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::not_found("Environment not found", CODE_ENVIRONMENT_NOT_FOUND))?;

        Ok(project_id)
    }

    pub async fn get_secret_env_id(&self, secret_id: Uuid) -> Result<Uuid, AppError> {
        let env_id: Uuid = sqlx::query_scalar("SELECT environment_id FROM secrets WHERE id = $1")
            .bind(secret_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| AppError::not_found("Secret not found", CODE_SECRET_NOT_FOUND))?;
        Ok(env_id)
    }

    pub async fn get_token_env_id(&self, token_id: Uuid) -> Result<Uuid, AppError> {
        let env_id: Uuid = sqlx::query_scalar("SELECT environment_id FROM service_tokens WHERE id = $1")
            .bind(token_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| AppError::not_found("Service token not found", CODE_TOKEN_NOT_FOUND))?;
        Ok(env_id)
    }

    // -----------------------------------------------------------------------
    // User Management Queries
    // -----------------------------------------------------------------------

    pub async fn find_user_by_identifier(&self, identifier: &str) -> Result<Option<(Uuid, String, String, String, String, bool)>, AppError> {
        let row: Option<(Uuid, String, String, String, String, bool)> = sqlx::query_as(
            "SELECT id, name, email, password, role, is_active FROM users WHERE email = $1 OR name = $1 LIMIT 1"
        )
        .bind(identifier)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(row)
    }

    pub async fn find_user_by_id(&self, id: Uuid) -> Result<Option<(Uuid, String, String, String, String, bool)>, AppError> {
        let row: Option<(Uuid, String, String, String, String, bool)> = sqlx::query_as(
            "SELECT id, name, email, password, role, is_active FROM users WHERE id = $1 LIMIT 1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(row)
    }

    pub async fn update_user_password(&self, id: Uuid, password_hash: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE users SET password = $1, updated_at = NOW() WHERE id = $2")
            .bind(password_hash)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;
        Ok(())
    }

    pub async fn count_admins(&self) -> Result<i64, AppError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE role = 'admin'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(count)
    }

    pub async fn insert_seeded_admin(
        &self,
        name: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<bool, AppError> {
        let res = sqlx::query(
            "INSERT INTO users (name, email, password, role, is_active)
             VALUES ($1, $2, $3, 'admin', true)
             ON CONFLICT (email) DO NOTHING"
        )
        .bind(name)
        .bind(email)
        .bind(password_hash)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(res.rows_affected() > 0)
    }

    pub async fn list_users_with_projects(&self) -> Result<Vec<UserSummary>, AppError> {
        let users: Vec<(Uuid, String, String, String, bool, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, name, email, role, is_active, created_at FROM users ORDER BY created_at ASC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let mut summaries = Vec::with_capacity(users.len());
        for (id, name, email, role, is_active, created_at) in users {
            let assigned: Vec<AssignedProjectInfo> = sqlx::query_as(
                "SELECT pm.project_id, p.name AS project_name, p.group_id, g.name AS group_name, pm.role
                 FROM project_members pm
                 INNER JOIN projects p ON p.id = pm.project_id
                 LEFT JOIN project_groups g ON g.id = p.group_id
                 WHERE pm.user_id = $1
                 ORDER BY p.name ASC"
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::Database)?;

            summaries.push(UserSummary {
                id,
                name,
                email,
                role,
                is_active,
                created_at,
                assigned_projects: assigned,
            });
        }

        Ok(summaries)
    }

    pub async fn create_user(
        &self,
        name: &str,
        email: &str,
        password_hash: &str,
        role: &str,
    ) -> Result<UserSummary, AppError> {
        let (id, created_at): (Uuid, DateTime<Utc>) = sqlx::query_as(
            "INSERT INTO users (name, email, password, role) VALUES ($1, $2, $3, $4) RETURNING id, created_at"
        )
        .bind(name)
        .bind(email)
        .bind(password_hash)
        .bind(role)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(UserSummary {
            id,
            name: name.to_string(),
            email: email.to_string(),
            role: role.to_string(),
            is_active: true,
            created_at,
            assigned_projects: Vec::new(),
        })
    }

    pub async fn update_user(
        &self,
        id: Uuid,
        name: Option<&str>,
        email: Option<&str>,
        role: Option<&str>,
        is_active: Option<bool>,
        password_hash: Option<&str>,
    ) -> Result<(), AppError> {
        if let Some(n) = name {
            sqlx::query("UPDATE users SET name = $1, updated_at = NOW() WHERE id = $2")
                .bind(n).bind(id).execute(&self.pool).await.map_err(AppError::Database)?;
        }
        if let Some(e) = email {
            sqlx::query("UPDATE users SET email = $1, updated_at = NOW() WHERE id = $2")
                .bind(e).bind(id).execute(&self.pool).await.map_err(AppError::Database)?;
        }
        if let Some(r) = role {
            sqlx::query("UPDATE users SET role = $1, updated_at = NOW() WHERE id = $2")
                .bind(r).bind(id).execute(&self.pool).await.map_err(AppError::Database)?;
        }
        if let Some(a) = is_active {
            sqlx::query("UPDATE users SET is_active = $1, updated_at = NOW() WHERE id = $2")
                .bind(a).bind(id).execute(&self.pool).await.map_err(AppError::Database)?;
        }
        if let Some(p) = password_hash {
            sqlx::query("UPDATE users SET password = $1, updated_at = NOW() WHERE id = $2")
                .bind(p).bind(id).execute(&self.pool).await.map_err(AppError::Database)?;
        }
        Ok(())
    }

    pub async fn delete_user(&self, id: Uuid) -> Result<(), AppError> {
        let res = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        if res.rows_affected() == 0 {
            return Err(AppError::not_found("User not found", "USER_NOT_FOUND"));
        }
        Ok(())
    }

    pub async fn assign_user_to_project(
        &self,
        user_id: Uuid,
        project_id: Uuid,
        role: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO project_members (user_id, project_id, role)
             VALUES ($1, $2, $3)
             ON CONFLICT (user_id, project_id) DO UPDATE SET role = EXCLUDED.role"
        )
        .bind(user_id)
        .bind(project_id)
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(())
    }

    pub async fn remove_user_from_project(
        &self,
        user_id: Uuid,
        project_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query("DELETE FROM project_members WHERE user_id = $1 AND project_id = $2")
            .bind(user_id)
            .bind(project_id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Project Groups (Ecosystem) Queries
    // -----------------------------------------------------------------------

    pub async fn list_groups(&self) -> Result<Vec<ProjectGroupWithCount>, AppError> {
        let rows: Vec<(Uuid, String, String, Option<String>, i64, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT g.id, g.name, g.slug, g.description,
                    COALESCE(COUNT(p.id), 0)::BIGINT AS projects_count,
                    g.created_at, g.updated_at
             FROM project_groups g
             LEFT JOIN projects p ON p.group_id = g.id
             GROUP BY g.id
             ORDER BY g.name ASC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(rows.into_iter().map(|(id, name, slug, description, projects_count, created_at, updated_at)| {
            ProjectGroupWithCount {
                id,
                name,
                slug,
                description,
                projects_count,
                created_at,
                updated_at,
            }
        }).collect())
    }

    pub async fn find_or_create_group_by_name(&self, name: &str) -> Result<Uuid, AppError> {
        let trimmed = name.trim();
        let slug = trimmed
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();

        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO project_groups (name, slug)
             VALUES ($1, $2)
             ON CONFLICT (slug) DO UPDATE SET name = EXCLUDED.name
             RETURNING id"
        )
        .bind(trimmed)
        .bind(&slug)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(id)
    }

    pub async fn create_group(
        &self,
        name: &str,
        slug: &str,
        description: Option<&str>,
    ) -> Result<ProjectGroup, AppError> {
        let group: ProjectGroup = sqlx::query_as(
            "INSERT INTO project_groups (name, slug, description)
             VALUES ($1, $2, $3)
             RETURNING id, name, slug, description, created_at, updated_at"
        )
        .bind(name)
        .bind(slug)
        .bind(description)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(group)
    }

    pub async fn update_group(
        &self,
        id: Uuid,
        name: Option<&str>,
        slug: Option<&str>,
        description: Option<&str>,
    ) -> Result<ProjectGroup, AppError> {
        if let Some(n) = name {
            sqlx::query("UPDATE project_groups SET name = $1, updated_at = NOW() WHERE id = $2")
                .bind(n).bind(id).execute(&self.pool).await.map_err(AppError::Database)?;
        }
        if let Some(s) = slug {
            sqlx::query("UPDATE project_groups SET slug = $1, updated_at = NOW() WHERE id = $2")
                .bind(s).bind(id).execute(&self.pool).await.map_err(AppError::Database)?;
        }
        if let Some(d) = description {
            sqlx::query("UPDATE project_groups SET description = $1, updated_at = NOW() WHERE id = $2")
                .bind(d).bind(id).execute(&self.pool).await.map_err(AppError::Database)?;
        }

        let updated: ProjectGroup = sqlx::query_as(
            "SELECT id, name, slug, description, created_at, updated_at FROM project_groups WHERE id = $1"
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(updated)
    }

    pub async fn delete_group(&self, id: Uuid) -> Result<(), AppError> {
        let res = sqlx::query("DELETE FROM project_groups WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        if res.rows_affected() == 0 {
            return Err(AppError::not_found("Project group not found", "GROUP_NOT_FOUND"));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Audit Logging Queries
    // -----------------------------------------------------------------------

    pub async fn get_secret_details(&self, secret_id: Uuid) -> Result<Option<(Uuid, String)>, AppError> {
        let row: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT environment_id, key FROM secrets WHERE id = $1"
        )
        .bind(secret_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(row)
    }

    pub async fn get_token_details(&self, token_id: Uuid) -> Result<Option<(Uuid, String, String)>, AppError> {
        let row: Option<(Uuid, String, String)> = sqlx::query_as(
            "SELECT environment_id, name, token_prefix FROM service_tokens WHERE id = $1"
        )
        .bind(token_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(row)
    }

    pub async fn record_audit_log(
        &self,
        env_id: Uuid,
        project_id: Option<Uuid>,
        user_id: Option<Uuid>,
        user_email: &str,
        user_name: &str,
        action: &str,
        summary: &str,
        affected_keys: &[String],
        details: serde_json::Value,
        ip_address: Option<&str>,
    ) -> Result<AuditLog, AppError> {
        let row: AuditLog = sqlx::query_as(
            "INSERT INTO audit_logs (environment_id, project_id, user_id, user_email, user_name, action, summary, affected_keys, details, ip_address, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
             RETURNING id, environment_id, project_id, user_id, user_email, user_name, action, summary, affected_keys, details, ip_address, created_at"
        )
        .bind(env_id)
        .bind(project_id)
        .bind(user_id)
        .bind(user_email)
        .bind(user_name)
        .bind(action)
        .bind(summary)
        .bind(affected_keys)
        .bind(details)
        .bind(ip_address)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(row)
    }

    pub async fn list_audit_logs(&self, env_id: Uuid, limit: i64) -> Result<Vec<AuditLog>, AppError> {
        let rows: Vec<AuditLog> = sqlx::query_as(
            "SELECT id, environment_id, project_id, user_id, user_email, user_name, action, summary, affected_keys, details, ip_address, created_at
             FROM audit_logs
             WHERE environment_id = $1
             ORDER BY created_at DESC
             LIMIT $2"
        )
        .bind(env_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(rows)
    }

    pub async fn create_webhook(
        &self,
        project_id: Uuid,
        environment_id: Option<Uuid>,
        name: &str,
        url: &str,
        secret_token: Option<&str>,
        events: serde_json::Value,
    ) -> Result<Webhook, AppError> {
        let row: Webhook = sqlx::query_as(
            "INSERT INTO webhooks (project_id, environment_id, name, url, secret_token, events)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id, project_id, environment_id, name, url, secret_token, events, is_active, last_triggered_at, last_status_code, created_at, updated_at"
        )
        .bind(project_id)
        .bind(environment_id)
        .bind(name)
        .bind(url)
        .bind(secret_token)
        .bind(events)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(row)
    }

    pub async fn get_webhook(&self, id: Uuid) -> Result<Webhook, AppError> {
        let row: Webhook = sqlx::query_as(
            "SELECT id, project_id, environment_id, name, url, secret_token, events, is_active, last_triggered_at, last_status_code, created_at, updated_at
             FROM webhooks
             WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::not_found("Webhook not found", "WEBHOOK_NOT_FOUND"))?;

        Ok(row)
    }

    pub async fn toggle_webhook_active(&self, id: Uuid) -> Result<bool, AppError> {
        let is_active: bool = sqlx::query_scalar(
            "UPDATE webhooks
             SET is_active = NOT is_active, updated_at = NOW()
             WHERE id = $1
             RETURNING is_active"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::not_found("Webhook not found", "WEBHOOK_NOT_FOUND"))?;

        Ok(is_active)
    }

    pub async fn list_webhooks_by_project(&self, project_id: Uuid) -> Result<Vec<Webhook>, AppError> {
        let rows: Vec<Webhook> = sqlx::query_as(
            "SELECT id, project_id, environment_id, name, url, secret_token, events, is_active, last_triggered_at, last_status_code, created_at, updated_at
             FROM webhooks
             WHERE project_id = $1
             ORDER BY created_at DESC"
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(rows)
    }

    pub async fn list_active_webhooks_for_env(
        &self,
        project_id: Uuid,
        env_id: Uuid,
    ) -> Result<Vec<Webhook>, AppError> {
        let rows: Vec<Webhook> = sqlx::query_as(
            "SELECT id, project_id, environment_id, name, url, secret_token, events, is_active, last_triggered_at, last_status_code, created_at, updated_at
             FROM webhooks
             WHERE project_id = $1
               AND is_active = true
               AND (environment_id IS NULL OR environment_id = $2)
             ORDER BY created_at ASC"
        )
        .bind(project_id)
        .bind(env_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(rows)
    }

    pub async fn delete_webhook(&self, id: Uuid) -> Result<(), AppError> {
        sqlx::query("DELETE FROM webhooks WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        Ok(())
    }

    pub async fn update_webhook_status(&self, id: Uuid, status_code: i32) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE webhooks
             SET last_triggered_at = NOW(), last_status_code = $1, updated_at = NOW()
             WHERE id = $2"
        )
        .bind(status_code)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;

        Ok(())
    }
}

