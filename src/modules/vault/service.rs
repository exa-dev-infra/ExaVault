use chrono::{Duration, Utc};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    constants::codes::*,
    modules::vault::{dto::*, repository::VaultRepository},
    pkg::{
        crypto::{
            decrypt_secret, encrypt_secret, generate_service_token, hash_service_token,
        },
        errors::AppError,
    },
};

#[derive(Clone)]
pub struct VaultService {
    repo: Arc<VaultRepository>,
    master_key: [u8; 32],
}

impl VaultService {
    pub fn new(repo: Arc<VaultRepository>, master_key: [u8; 32]) -> Self {
        Self { repo, master_key }
    }

    pub fn repository(&self) -> &VaultRepository {
        &self.repo
    }

    pub async fn list_projects(&self) -> Result<Vec<ProjectWithEnvironments>, AppError> {
        self.repo.list_projects().await
    }

    pub async fn create_project(&self, req: CreateProjectRequest) -> Result<ProjectWithEnvironments, AppError> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err(AppError::bad_request("Project name cannot be empty", "INVALID_NAME"));
        }

        let slug = req.slug.unwrap_or_else(|| {
            name.to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>()
        });
        let slug = slug.trim_matches('-').to_string();

        let group_id = match req.group_id {
            Some(gid) => Some(gid),
            None => match req.group_name {
                Some(ref gname) if !gname.trim().is_empty() => {
                    let gid = self.repo.find_or_create_group_by_name(gname).await?;
                    Some(gid)
                }
                _ => None,
            },
        };

        self.repo
            .create_project(name, &slug, req.description.as_deref(), group_id)
            .await
    }

    pub async fn delete_project(&self, id: Uuid) -> Result<(), AppError> {
        self.repo.delete_project(id).await
    }

    pub async fn list_secrets_decrypted(&self, env_id: Uuid) -> Result<Vec<DecryptedSecret>, AppError> {
        let records = self.repo.list_secrets(env_id).await?;
        let mut decrypted = Vec::with_capacity(records.len());

        for record in records {
            let plaintext = match decrypt_secret(
                &record.encrypted_value,
                &record.iv,
                &record.auth_tag,
                &self.master_key,
            ) {
                Ok(val) => val,
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        secret_id = %record.id,
                        key = %record.key,
                        "Failed to decrypt secret"
                    );
                    "[DECRYPTION_FAILED]".to_string()
                }
            };

            decrypted.push(DecryptedSecret {
                id: record.id,
                environment_id: record.environment_id,
                key: record.key,
                value: plaintext,
                comment: record.comment,
                updated_at: record.updated_at,
            });
        }

        Ok(decrypted)
    }

    pub async fn bulk_upsert_secrets(
        &self,
        env_id: Uuid,
        items: Vec<UpsertSecretItem>,
        user: Option<&VaultAuthUser>,
    ) -> Result<(), AppError> {
        let env = self.repo.get_environment(env_id).await?;
        let mut encrypted_items = Vec::with_capacity(items.len());
        let mut affected_keys = Vec::with_capacity(items.len());

        for item in items {
            let key = item.key.trim();
            if key.is_empty() {
                continue;
            }

            affected_keys.push(key.to_string());
            // Generate unique 12-byte IV for every single secret write/update
            let (enc_val, iv, auth_tag) = encrypt_secret(&item.value, &self.master_key)?;
            encrypted_items.push((key.to_string(), enc_val, iv, auth_tag, item.comment));
        }

        self.repo.bulk_upsert_secrets(env_id, encrypted_items).await?;

        // Audit Trail Entry
        let (u_id, u_email, u_name) = match user {
            Some(u) => (Some(u.id), u.email.as_str(), u.username.as_str()),
            None => (None, "system", "System / CLI"),
        };

        let summary = format!(
            "Saved {} secret variables in {} ({})",
            affected_keys.len(),
            env.name,
            env.slug
        );

        let details = serde_json::json!({
            "count": affected_keys.len(),
            "environment_slug": env.slug,
            "environment_name": env.name,
        });

        let _ = self.repo.record_audit_log(
            env_id,
            Some(env.project_id),
            u_id,
            u_email,
            u_name,
            "SAVE_SECRETS",
            &summary,
            &affected_keys,
            details,
            None,
        ).await;

        Ok(())
    }

    pub async fn delete_secret(&self, id: Uuid, user: Option<&VaultAuthUser>) -> Result<(), AppError> {
        let secret_details = self.repo.get_secret_details(id).await?;
        self.repo.delete_secret(id).await?;

        if let Some((env_id, key)) = secret_details {
            let env = self.repo.get_environment(env_id).await.ok();
            let (u_id, u_email, u_name) = match user {
                Some(u) => (Some(u.id), u.email.as_str(), u.username.as_str()),
                None => (None, "system", "System / CLI"),
            };

            let summary = format!("Deleted secret variable '{}'", key);
            let details = serde_json::json!({ "deleted_key": key });

            let _ = self.repo.record_audit_log(
                env_id,
                env.as_ref().map(|e| e.project_id),
                u_id,
                u_email,
                u_name,
                "DELETE_SECRET",
                &summary,
                &[key],
                details,
                None,
            ).await;
        }

        Ok(())
    }

    pub async fn list_tokens(&self, env_id: Uuid) -> Result<Vec<ServiceTokenListItem>, AppError> {
        self.repo.list_tokens(env_id).await
    }

    pub async fn create_service_token(
        &self,
        env_id: Uuid,
        req: CreateServiceTokenRequest,
        user: Option<&VaultAuthUser>,
    ) -> Result<NewServiceTokenResponse, AppError> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err(AppError::bad_request("Token name cannot be empty", "INVALID_NAME"));
        }

        let (raw_token, token_prefix, token_hash) = generate_service_token();

        let expires_at = req.expires_in_days.and_then(|days| {
            if days > 0 {
                Some(Utc::now() + Duration::days(days))
            } else {
                None
            }
        });

        let token = self
            .repo
            .create_token(env_id, name, &token_prefix, &token_hash, expires_at)
            .await?;

        let env = self.repo.get_environment(env_id).await.ok();
        let (u_id, u_email, u_name) = match user {
            Some(u) => (Some(u.id), u.email.as_str(), u.username.as_str()),
            None => (None, "system", "System / CLI"),
        };

        let summary = format!("Generated service token '{}' ({})", token.name, token.token_prefix);
        let details = serde_json::json!({
            "token_name": token.name,
            "token_prefix": token.token_prefix,
            "expires_at": token.expires_at,
        });

        let _ = self.repo.record_audit_log(
            env_id,
            env.as_ref().map(|e| e.project_id),
            u_id,
            u_email,
            u_name,
            "CREATE_TOKEN",
            &summary,
            &[],
            details,
            None,
        ).await;

        // Raw token is returned only once
        Ok(NewServiceTokenResponse {
            id: token.id,
            environment_id: token.environment_id,
            name: token.name,
            token: raw_token,
            token_prefix: token.token_prefix,
            expires_at: token.expires_at,
            created_at: token.created_at,
        })
    }

    pub async fn delete_token(&self, id: Uuid, user: Option<&VaultAuthUser>) -> Result<(), AppError> {
        let token_details = self.repo.get_token_details(id).await?;
        self.repo.delete_token(id).await?;

        if let Some((env_id, name, prefix)) = token_details {
            let env = self.repo.get_environment(env_id).await.ok();
            let (u_id, u_email, u_name) = match user {
                Some(u) => (Some(u.id), u.email.as_str(), u.username.as_str()),
                None => (None, "system", "System / CLI"),
            };

            let summary = format!("Revoked service token '{}' ({})", name, prefix);
            let details = serde_json::json!({
                "token_name": name,
                "token_prefix": prefix,
            });

            let _ = self.repo.record_audit_log(
                env_id,
                env.as_ref().map(|e| e.project_id),
                u_id,
                u_email,
                u_name,
                "REVOKE_TOKEN",
                &summary,
                &[],
                details,
                None,
            ).await;
        }

        Ok(())
    }

    pub async fn list_audit_logs(&self, env_id: Uuid, limit: Option<i64>) -> Result<Vec<AuditLog>, AppError> {
        let limit = limit.unwrap_or(50).clamp(1, 200);
        self.repo.list_audit_logs(env_id, limit).await
    }

    /// Verifies Bearer service token and exports decrypted secrets in requested format.
    /// Returns (formatted_content, content_type)
    pub async fn export_secrets(
        &self,
        raw_token: &str,
        format: &str,
    ) -> Result<(String, &'static str), AppError> {
        let trimmed_token = raw_token.trim();
        if !trimmed_token.starts_with("exv_") {
            return Err(AppError::unauthorized(
                "Invalid service token format. Must start with 'exv_'",
                CODE_UNAUTHORIZED,
            ));
        }

        let token_hash = hash_service_token(trimmed_token);
        let (token, _env) = self.repo.find_token_by_hash(&token_hash).await?;

        // Check expiration
        if let Some(exp) = token.expires_at {
            if Utc::now() > exp {
                return Err(AppError::unauthorized("Service token has expired", CODE_TOKEN_EXPIRED));
            }
        }

        // Update last used timestamp
        let _ = self.repo.touch_token_last_used(token.id).await;

        // Fetch & decrypt secrets
        let secrets = self.list_secrets_decrypted(token.environment_id).await?;

        match format.to_lowercase().as_str() {
            "json" => {
                let mut map = serde_json::Map::new();
                for s in secrets {
                    map.insert(s.key, json!(s.value));
                }
                let content = serde_json::to_string_pretty(&map)
                    .map_err(|e| AppError::internal(format!("JSON serialization error: {}", e)))?;
                Ok((content, "application/json; charset=utf-8"))
            }
            "shell" => {
                let mut lines = Vec::new();
                for s in secrets {
                    if let Some(comment) = &s.comment {
                        if !comment.trim().is_empty() {
                            lines.push(format!("# {}", comment.trim()));
                        }
                    }
                    let escaped = escape_shell_string(&s.value);
                    lines.push(format!("export {}=\"{}\"", s.key, escaped));
                }
                let content = lines.join("\n") + "\n";
                Ok((content, "text/plain; charset=utf-8"))
            }
            _ => {
                // Default: dotenv
                let mut lines = Vec::new();
                for s in secrets {
                    if let Some(comment) = &s.comment {
                        if !comment.trim().is_empty() {
                            lines.push(format!("# {}", comment.trim()));
                        }
                    }
                    let escaped = escape_dotenv_string(&s.value);
                    lines.push(format!("{}=\"{}\"", s.key, escaped));
                }
                let content = lines.join("\n") + "\n";
                Ok((content, "text/plain; charset=utf-8"))
            }
        }
    }

    // -----------------------------------------------------------------------
    // RBAC & Scoped Project Access
    // -----------------------------------------------------------------------

    pub async fn list_projects_for_user(
        &self,
        user: &VaultAuthUser,
    ) -> Result<Vec<ProjectWithEnvironments>, AppError> {
        if user.role == "admin" {
            self.repo.list_projects().await
        } else {
            self.repo.list_projects_for_user(user.id).await
        }
    }

    pub async fn check_project_permission(
        &self,
        user: &VaultAuthUser,
        project_id: Uuid,
        required_level: &str, // "read", "editor", "admin"
    ) -> Result<String, AppError> {
        if user.role == "admin" {
            return Ok("admin".to_string());
        }

        let role = self
            .repo
            .get_user_project_role(user.id, project_id)
            .await?
            .ok_or_else(|| AppError::forbidden("You do not have access to this project", CODE_FORBIDDEN))?;

        match required_level {
            "admin" => {
                if role != "admin" {
                    return Err(AppError::forbidden("Project administrator privileges required", CODE_FORBIDDEN));
                }
            }
            "editor" => {
                if role == "viewer" {
                    return Err(AppError::forbidden("Viewer role has read-only access. Modification is forbidden.", CODE_FORBIDDEN));
                }
            }
            _ => {} // "read" allows viewer, editor, and admin
        }

        Ok(role)
    }

    pub async fn check_env_permission(
        &self,
        user: &VaultAuthUser,
        env_id: Uuid,
        required_level: &str,
    ) -> Result<String, AppError> {
        let project_id = self.repo.get_project_id_by_env_id(env_id).await?;
        self.check_project_permission(user, project_id, required_level).await
    }

    // -----------------------------------------------------------------------
    // User Management Methods
    // -----------------------------------------------------------------------

    pub async fn authenticate_user(
        &self,
        identifier: &str,
        password: &str,
    ) -> Result<Option<VaultAuthUser>, AppError> {
        let user = self.repo.find_user_by_identifier(identifier).await?;
        if let Some((id, name, email, password_hash, role, is_active)) = user {
            if !is_active {
                return Err(AppError::unauthorized("User account is disabled", CODE_UNAUTHORIZED));
            }
            if crate::pkg::auth::password::verify_password(password, &password_hash).unwrap_or(false) {
                return Ok(Some(VaultAuthUser {
                    id,
                    username: name,
                    email,
                    role,
                }));
            }
        }
        Ok(None)
    }

    pub async fn list_users(&self) -> Result<Vec<UserSummary>, AppError> {
        self.repo.list_users_with_projects().await
    }

    pub async fn create_user(&self, req: CreateVaultUserRequest) -> Result<UserSummary, AppError> {
        let name = req.name.trim();
        let email = req.email.trim();
        if name.is_empty() || email.is_empty() {
            return Err(AppError::bad_request("Name and email are required", "INVALID_USER_DATA"));
        }

        let password_hash = crate::pkg::auth::password::hash_password(&req.password)?;
        let role = req.role.unwrap_or_else(|| "user".to_string());

        self.repo.create_user(name, email, &password_hash, &role).await
    }

    pub async fn update_user(&self, id: Uuid, req: UpdateVaultUserRequest) -> Result<(), AppError> {
        let password_hash = match req.password {
            Some(ref p) if !p.trim().is_empty() => Some(crate::pkg::auth::password::hash_password(p)?),
            _ => None,
        };

        self.repo
            .update_user(
                id,
                req.name.as_deref(),
                req.email.as_deref(),
                req.role.as_deref(),
                req.is_active,
                password_hash.as_deref(),
            )
            .await
    }

    pub async fn delete_user(&self, id: Uuid) -> Result<(), AppError> {
        self.repo.delete_user(id).await
    }

    pub async fn assign_project(
        &self,
        user_id: Uuid,
        project_id: Uuid,
        role: &str,
    ) -> Result<(), AppError> {
        let valid_role = match role.to_lowercase().as_str() {
            "admin" => "admin",
            "editor" => "editor",
            "viewer" => "viewer",
            _ => "editor",
        };
        self.repo.assign_user_to_project(user_id, project_id, valid_role).await
    }

    pub async fn remove_project(&self, user_id: Uuid, project_id: Uuid) -> Result<(), AppError> {
        self.repo.remove_user_from_project(user_id, project_id).await
    }

    // -----------------------------------------------------------------------
    // Project Groups (Ecosystem) Management
    // -----------------------------------------------------------------------

    pub async fn list_groups(&self) -> Result<Vec<ProjectGroupWithCount>, AppError> {
        self.repo.list_groups().await
    }

    pub async fn create_group(&self, req: CreateProjectGroupRequest) -> Result<ProjectGroup, AppError> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err(AppError::bad_request("Group name cannot be empty", "INVALID_NAME"));
        }
        let slug = req.slug.unwrap_or_else(|| {
            name.to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>()
        });
        let slug = slug.trim_matches('-').to_string();

        self.repo.create_group(name, &slug, req.description.as_deref()).await
    }

    pub async fn update_group(&self, id: Uuid, req: UpdateProjectGroupRequest) -> Result<ProjectGroup, AppError> {
        self.repo
            .update_group(id, req.name.as_deref(), req.slug.as_deref(), req.description.as_deref())
            .await
    }

    pub async fn delete_group(&self, id: Uuid) -> Result<(), AppError> {
        self.repo.delete_group(id).await
    }
}

/// Escapes values for standard .env files
fn escape_dotenv_string(val: &str) -> String {
    val.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Escapes values for bash/sh export commands
fn escape_shell_string(val: &str) -> String {
    val.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

