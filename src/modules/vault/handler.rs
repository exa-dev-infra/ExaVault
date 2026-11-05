use axum::{
    extract::{Path, Query, Request, State},
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
// Admin Authentication Middleware
// ---------------------------------------------------------------------------

pub async fn require_admin_auth(
    State(ctx): State<VaultContext>,
    jar: CookieJar,
    req: Request,
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
        AppError::unauthorized("Admin authentication required", CODE_UNAUTHORIZED)
    })?;

    let _claims = verify_jwt(&token, &ctx.jwt_secret)?;
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
    if payload.username != ctx.admin_username
        || !verify_admin_password(&payload.password, &ctx.admin_password)
    {
        return Err(AppError::unauthorized("Invalid username or password", CODE_INVALID_CREDENTIALS));
    }

    let token = create_jwt(&payload.username, &ctx.jwt_secret)?;

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
                username: payload.username,
            },
            "Authentication successful",
        ),
    ))
}

pub async fn me_handler(
    State(ctx): State<VaultContext>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let token = if let Some(auth_header) = headers.get(header::AUTHORIZATION) {
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

    let token = token.ok_or_else(|| AppError::unauthorized("Not authenticated", CODE_UNAUTHORIZED))?;
    let claims = verify_jwt(&token, &ctx.jwt_secret)?;

    Ok(BaseResponse::success(
        json!({
            "username": claims.sub,
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
) -> Result<impl IntoResponse, AppError> {
    let projects = ctx.service.list_projects().await?;
    Ok(BaseResponse::success(projects, "Projects retrieved"))
}

pub async fn create_project_handler(
    State(ctx): State<VaultContext>,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<(StatusCode, impl IntoResponse), AppError> {
    let created = ctx.service.create_project(payload).await?;
    Ok((StatusCode::CREATED, BaseResponse::success(created, "Project created successfully")))
}

pub async fn delete_project_handler(
    State(ctx): State<VaultContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    ctx.service.delete_project(id).await?;
    Ok(BaseResponse::success(json!({ "id": id }), "Project deleted successfully"))
}

// ---------------------------------------------------------------------------
// Secrets Handlers
// ---------------------------------------------------------------------------

pub async fn list_secrets_handler(
    State(ctx): State<VaultContext>,
    Path(env_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let secrets = ctx.service.list_secrets_decrypted(env_id).await?;
    Ok(BaseResponse::success(secrets, "Secrets retrieved"))
}

pub async fn bulk_upsert_secrets_handler(
    State(ctx): State<VaultContext>,
    Path(env_id): Path<Uuid>,
    Json(payload): Json<Vec<UpsertSecretItem>>,
) -> Result<impl IntoResponse, AppError> {
    ctx.service.bulk_upsert_secrets(env_id, payload).await?;
    Ok(BaseResponse::success(json!({}), "Secrets saved successfully"))
}

pub async fn delete_secret_handler(
    State(ctx): State<VaultContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    ctx.service.delete_secret(id).await?;
    Ok(BaseResponse::success(json!({ "id": id }), "Secret deleted successfully"))
}

// ---------------------------------------------------------------------------
// Service Tokens Handlers
// ---------------------------------------------------------------------------

pub async fn list_tokens_handler(
    State(ctx): State<VaultContext>,
    Path(env_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let tokens = ctx.service.list_tokens(env_id).await?;
    Ok(BaseResponse::success(tokens, "Service tokens retrieved"))
}

pub async fn create_token_handler(
    State(ctx): State<VaultContext>,
    Path(env_id): Path<Uuid>,
    Json(payload): Json<CreateServiceTokenRequest>,
) -> Result<(StatusCode, impl IntoResponse), AppError> {
    let token = ctx.service.create_service_token(env_id, payload).await?;
    Ok((StatusCode::CREATED, BaseResponse::success(token, "Service token created")))
}

pub async fn delete_token_handler(
    State(ctx): State<VaultContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    ctx.service.delete_token(id).await?;
    Ok(BaseResponse::success(json!({ "id": id }), "Service token revoked"))
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

    let (content, content_type) = ctx.service.export_secrets(raw_token, &format).await?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(axum::body::Body::from(content))
        .map_err(|e| AppError::internal(format!("Response build error: {}", e)))?;

    Ok(response)
}

