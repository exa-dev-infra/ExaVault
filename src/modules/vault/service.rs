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

        self.repo
            .create_project(name, &slug, req.description.as_deref(), req.group_name.as_deref())
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
    ) -> Result<(), AppError> {
        let mut encrypted_items = Vec::with_capacity(items.len());

        for item in items {
            let key = item.key.trim();
            if key.is_empty() {
                continue;
            }

            // Generate unique 12-byte IV for every single secret write/update
            let (enc_val, iv, auth_tag) = encrypt_secret(&item.value, &self.master_key)?;
            encrypted_items.push((key.to_string(), enc_val, iv, auth_tag, item.comment));
        }

        self.repo.bulk_upsert_secrets(env_id, encrypted_items).await
    }

    pub async fn delete_secret(&self, id: Uuid) -> Result<(), AppError> {
        self.repo.delete_secret(id).await
    }

    pub async fn list_tokens(&self, env_id: Uuid) -> Result<Vec<ServiceTokenListItem>, AppError> {
        self.repo.list_tokens(env_id).await
    }

    pub async fn create_service_token(
        &self,
        env_id: Uuid,
        req: CreateServiceTokenRequest,
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

    pub async fn delete_token(&self, id: Uuid) -> Result<(), AppError> {
        self.repo.delete_token(id).await
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

