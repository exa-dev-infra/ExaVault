use axum::{
    extract::{Extension, Path, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    constants::codes::*,
    modules::vault::{dto::*, service::VaultService},
    pkg::{
        crypto::{create_jwt, verify_admin_password, verify_jwt},
        errors::AppError,
        response::BaseResponse,
    },
};

#[derive(Clone)]
pub struct VaultContext {
    pub service: Arc<VaultService>,
    pub admin_username: String,
    pub admin_password: String,
    pub jwt_secret: String,
}

#[derive(Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>,
}

// ---------------------------------------------------------------------------
// Authentication & RBAC Middleware
// ---------------------------------------------------------------------------

pub async fn require_admin_auth(
    State(ctx): State<VaultContext>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                Some(auth_str[7..].to_string())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        jar.get("exavault_session").map(|c| c.value().to_string())
    };

    let token = token.ok_or_else(|| {
        AppError::unauthorized("Authentication required", CODE_UNAUTHORIZED)
    })?;

    let claims = verify_jwt(&token, &ctx.jwt_secret)?;
    let user_id = Uuid::parse_str(&claims.sub).unwrap_or_else(|_| Uuid::nil());
    let auth_user = VaultAuthUser {
        id: user_id,
        username: claims.username,
        email: claims.email,
        role: claims.role,
    };

    req.extensions_mut().insert(auth_user);
    Ok(next.run(req).await)
}

// ---------------------------------------------------------------------------
// Auth Handlers
// ---------------------------------------------------------------------------

pub async fn login_handler(
    State(ctx): State<VaultContext>,
    jar: CookieJar,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, impl IntoResponse), AppError> {
    let (auth_user, username) = if payload.username == ctx.admin_username
        && verify_admin_password(&payload.password, &ctx.admin_password)
    {
        (
            VaultAuthUser {
                id: Uuid::nil(),
                username: payload.username.clone(),
                email: "admin@exavault.local".to_string(),
                role: "admin".to_string(),
            },
            payload.username,
        )
    } else if let Some(user) = ctx.service.authenticate_user(&payload.username, &payload.password).await? {
        let name = user.username.clone();
        (user, name)
    } else {
        return Err(AppError::unauthorized("Invalid username or password", CODE_INVALID_CREDENTIALS));
    };

    let token = create_jwt(
        &auth_user.id.to_string(),
        &auth_user.username,
        &auth_user.email,
        &auth_user.role,
        &ctx.jwt_secret,
    )?;

    let mut cookie = Cookie::new("exavault_session", token.clone());
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_max_age(time::Duration::days(7));

    let updated_jar = jar.add(cookie);

    Ok((
        updated_jar,
        BaseResponse::success(
            LoginResponse {
                token,
                username,
                role: auth_user.role,
            },
            "Authentication successful",
        ),
    ))
}

pub async fn me_handler(
    Extension(user): Extension<VaultAuthUser>,
) -> Result<impl IntoResponse, AppError> {
    Ok(BaseResponse::success(
        json!({
            "id": user.id,
            "username": user.username,
            "email": user.email,
            "role": user.role,
            "is_admin": user.role == "admin",
            "authenticated": true
        }),
        "Session valid",
    ))
}

pub async fn logout_handler(jar: CookieJar) -> (CookieJar, impl IntoResponse) {
    let mut cookie = Cookie::new("exavault_session", "");
    cookie.set_path("/");
    cookie.set_max_age(time::Duration::seconds(0));
    let updated_jar = jar.add(cookie);

    (updated_jar, BaseResponse::success(json!({}), "Logged out successfully"))
}

// ---------------------------------------------------------------------------
// Projects Handlers
// ---------------------------------------------------------------------------

pub async fn list_projects_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let projects = ctx.service.list_projects_for_user(&user).await?;
    Ok(BaseResponse::success(projects, "Projects retrieved"))
}

pub async fn create_project_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<(StatusCode, impl IntoResponse), AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Only administrators can create projects", CODE_FORBIDDEN));
    }
    let created = ctx.service.create_project(payload).await?;
    Ok((StatusCode::CREATED, BaseResponse::success(created, "Project created successfully")))
}

pub async fn delete_project_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Only administrators can delete projects", CODE_FORBIDDEN));
    }
    ctx.service.delete_project(id).await?;
    Ok(BaseResponse::success(json!({ "id": id }), "Project deleted successfully"))
}

// ---------------------------------------------------------------------------
// Project Groups Handlers (Ecosystem Management)
// ---------------------------------------------------------------------------

pub async fn list_groups_handler(
    State(ctx): State<VaultContext>,
) -> Result<impl IntoResponse, AppError> {
    let groups = ctx.service.list_groups().await?;
    Ok(BaseResponse::success(groups, "Groups retrieved successfully"))
}

pub async fn create_group_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Json(payload): Json<CreateProjectGroupRequest>,
) -> Result<(StatusCode, impl IntoResponse), AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Only administrators can create project groups", CODE_FORBIDDEN));
    }
    let created = ctx.service.create_group(payload).await?;
    Ok((StatusCode::CREATED, BaseResponse::success(created, "Group created successfully")))
}

pub async fn update_group_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateProjectGroupRequest>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Only administrators can update project groups", CODE_FORBIDDEN));
    }
    let updated = ctx.service.update_group(id, payload).await?;
    Ok(BaseResponse::success(updated, "Group updated successfully"))
}

pub async fn delete_group_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Only administrators can delete project groups", CODE_FORBIDDEN));
    }
    ctx.service.delete_group(id).await?;
    Ok(BaseResponse::success(json!({ "id": id }), "Group deleted successfully"))
}

// ---------------------------------------------------------------------------
// Secrets Handlers
// ---------------------------------------------------------------------------

pub async fn list_secrets_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(env_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let user_role = ctx.service.check_env_permission(&user, env_id, "read").await?;
    let secrets = ctx.service.list_secrets_decrypted(env_id).await?;
    Ok(BaseResponse::success(
        json!({
            "secrets": secrets,
            "user_role": user_role
        }),
        "Secrets retrieved",
    ))
}

pub async fn bulk_upsert_secrets_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(env_id): Path<Uuid>,
    Json(payload): Json<Vec<UpsertSecretItem>>,
) -> Result<impl IntoResponse, AppError> {
    ctx.service.check_env_permission(&user, env_id, "editor").await?;
    ctx.service.bulk_upsert_secrets(env_id, payload, Some(&user)).await?;
    Ok(BaseResponse::success(json!({}), "Secrets saved successfully"))
}

pub async fn delete_secret_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        let env_id = ctx.service.repository().get_secret_env_id(id).await?;
        ctx.service.check_env_permission(&user, env_id, "editor").await?;
    }
    ctx.service.delete_secret(id, Some(&user)).await?;
    Ok(BaseResponse::success(json!({ "id": id }), "Secret deleted successfully"))
}

// ---------------------------------------------------------------------------
// Service Tokens Handlers
// ---------------------------------------------------------------------------

pub async fn list_tokens_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(env_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    ctx.service.check_env_permission(&user, env_id, "read").await?;
    let tokens = ctx.service.list_tokens(env_id).await?;
    Ok(BaseResponse::success(tokens, "Service tokens retrieved"))
}

pub async fn create_token_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(env_id): Path<Uuid>,
    Json(payload): Json<CreateServiceTokenRequest>,
) -> Result<(StatusCode, impl IntoResponse), AppError> {
    ctx.service.check_env_permission(&user, env_id, "editor").await?;
    let token = ctx.service.create_service_token(env_id, payload, Some(&user)).await?;
    Ok((StatusCode::CREATED, BaseResponse::success(token, "Service token created")))
}

pub async fn delete_token_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        let env_id = ctx.service.repository().get_token_env_id(id).await?;
        ctx.service.check_env_permission(&user, env_id, "editor").await?;
    }
    ctx.service.delete_token(id, Some(&user)).await?;
    Ok(BaseResponse::success(json!({ "id": id }), "Service token revoked"))
}

// ---------------------------------------------------------------------------
// Audit Logs Handler
// ---------------------------------------------------------------------------

pub async fn list_audit_logs_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(env_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    ctx.service.check_env_permission(&user, env_id, "read").await?;
    let logs = ctx.service.list_audit_logs(env_id, Some(50)).await?;
    Ok(BaseResponse::success(logs, "Audit logs retrieved"))
}

// ---------------------------------------------------------------------------
// User Management Handlers (Admin Only)
// ---------------------------------------------------------------------------

pub async fn list_users_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Admin privileges required", CODE_FORBIDDEN));
    }
    let users = ctx.service.list_users().await?;
    Ok(BaseResponse::success(users, "Users retrieved successfully"))
}

pub async fn create_user_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Json(payload): Json<CreateVaultUserRequest>,
) -> Result<(StatusCode, impl IntoResponse), AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Admin privileges required", CODE_FORBIDDEN));
    }
    let created = ctx.service.create_user(payload).await?;
    Ok((StatusCode::CREATED, BaseResponse::success(created, "User created successfully")))
}

pub async fn update_user_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateVaultUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Admin privileges required", CODE_FORBIDDEN));
    }
    ctx.service.update_user(id, payload).await?;
    Ok(BaseResponse::success(json!({ "id": id }), "User updated successfully"))
}

pub async fn delete_user_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Admin privileges required", CODE_FORBIDDEN));
    }
    ctx.service.delete_user(id).await?;
    Ok(BaseResponse::success(json!({ "id": id }), "User deleted successfully"))
}

pub async fn assign_user_project_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path(id): Path<Uuid>,
    Json(payload): Json<AssignProjectRequest>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Admin privileges required", CODE_FORBIDDEN));
    }
    ctx.service.assign_project(id, payload.project_id, &payload.role).await?;
    Ok(BaseResponse::success(
        json!({ "user_id": id, "project_id": payload.project_id }),
        "Project assigned successfully",
    ))
}

pub async fn remove_user_project_handler(
    State(ctx): State<VaultContext>,
    Extension(user): Extension<VaultAuthUser>,
    Path((id, project_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    if user.role != "admin" {
        return Err(AppError::forbidden("Admin privileges required", CODE_FORBIDDEN));
    }
    ctx.service.remove_project(id, project_id).await?;
    Ok(BaseResponse::success(
        json!({ "user_id": id, "project_id": project_id }),
        "Project assignment removed",
    ))
}

// ---------------------------------------------------------------------------
// CI/CD & Remote Export Handler (`GET /api/v1/export`)
// ---------------------------------------------------------------------------

pub async fn export_secrets_handler(
    State(ctx): State<VaultContext>,
    Query(query): Query<ExportQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            AppError::unauthorized(
                "Missing Authorization header. Use 'Authorization: Bearer exv_...'",
                CODE_UNAUTHORIZED,
            )
        })?;

    if !auth_header.starts_with("Bearer ") {
        return Err(AppError::unauthorized(
            "Invalid authorization scheme. Bearer required.",
            CODE_UNAUTHORIZED,
        ));
    }

    let raw_token = &auth_header[7..];
    let format = query.format.unwrap_or_else(|| "dotenv".to_string());

    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.trim())
        });

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|h| h.to_str().ok());

    let (content, content_type) = ctx
        .service
        .export_secrets(raw_token, &format, client_ip, user_agent)
        .await?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(axum::body::Body::from(content))
        .map_err(|e| AppError::internal(format!("Response build error: {}", e)))?;

    Ok(response)
}

