use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectGroup {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectGroupWithCount {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub projects_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectGroupRequest {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProjectGroupRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub group_id: Option<Uuid>,
    pub group_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Environment {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectWithEnvironments {
    #[serde(flatten)]
    pub project: Project,
    pub environments: Vec<Environment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Secret {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub key: String,
    pub encrypted_value: String,
    pub iv: String,
    pub auth_tag: String,
    pub comment: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptedSecret {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub key: String,
    pub value: String,
    pub comment: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServiceToken {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub name: String,
    pub token_prefix: String,
    pub token_hash: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceTokenListItem {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub name: String,
    pub token_prefix: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub group_id: Option<Uuid>,
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSecretItem {
    pub key: String,
    pub value: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateServiceTokenRequest {
    pub name: String,
    pub expires_in_days: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewServiceTokenResponse {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub name: String,
    pub token: String, // Displayed raw only ONCE!
    pub token_prefix: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultAuthUser {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String, // "admin" or "user"
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AssignedProjectInfo {
    pub project_id: Uuid,
    pub project_name: String,
    pub group_id: Option<Uuid>,
    pub group_name: Option<String>,
    pub role: String, // "admin" | "editor" | "viewer"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummary {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub assigned_projects: Vec<AssignedProjectInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVaultUserRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    pub role: Option<String>, // default "user"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateVaultUserRequest {
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub is_active: Option<bool>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignProjectRequest {
    pub project_id: Uuid,
    pub role: String, // "admin", "editor", "viewer"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSecretsResponse {
    pub secrets: Vec<DecryptedSecret>,
    pub user_role: String, // "admin", "editor", "viewer"
}

