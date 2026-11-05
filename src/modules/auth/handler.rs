use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use std::sync::Arc;

use crate::{
    modules::auth::{
        dto::{ChangePasswordRequest, LoginRequest, RefreshTokenRequest, RegisterRequest},
        service::AuthService,
    },
    pkg::{
        auth::jwt::AuthenticatedUser, errors::AppError, response::BaseResponse,
        validation::ValidatedJson,
    },
};

const REFRESH_COOKIE_NAME: &str = "refresh_token";

fn create_refresh_cookie(token: &str, secure: bool) -> Cookie<'static> {
    Cookie::build((REFRESH_COOKIE_NAME, token.to_string()))
        .path("/api/v1/auth")
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build()
}

fn clear_refresh_cookie() -> Cookie<'static> {
    Cookie::build((REFRESH_COOKIE_NAME, ""))
        .path("/api/v1/auth")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(0))
        .build()
}

fn extract_client_info(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let ip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    (user_agent, ip)
}

pub async fn register_handler(
    State(service): State<Arc<AuthService>>,
    jar: CookieJar,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (ua, ip) = extract_client_info(&headers);
    let auth_res = service.register(req, ua.as_deref(), ip.as_deref()).await?;

    let cookie = create_refresh_cookie(&auth_res.tokens.refresh_token, false);
    let updated_jar = jar.add(cookie);

    let (status, body) = BaseResponse::created(auth_res, "User registered successfully");
    Ok((status, updated_jar, body))
}

pub async fn login_handler(
    State(service): State<Arc<AuthService>>,
    jar: CookieJar,
    headers: HeaderMap,
    ValidatedJson(req): ValidatedJson<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (ua, ip) = extract_client_info(&headers);
    let auth_res = service.login(req, ua.as_deref(), ip.as_deref()).await?;

    let cookie = create_refresh_cookie(&auth_res.tokens.refresh_token, false);
    let updated_jar = jar.add(cookie);

    let (status, body) = BaseResponse::success(auth_res, "Login successful");
    Ok((status, updated_jar, body))
}

pub async fn refresh_handler(
    State(service): State<Arc<AuthService>>,
    jar: CookieJar,
    headers: HeaderMap,
    axum::extract::Json(body): axum::extract::Json<RefreshTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let token = body
        .refresh_token
        .or_else(|| jar.get(REFRESH_COOKIE_NAME).map(|c| c.value().to_string()))
        .ok_or_else(AppError::refresh_token_required)?;

    let (ua, ip) = extract_client_info(&headers);
    let auth_res = service
        .refresh(&token, ua.as_deref(), ip.as_deref())
        .await?;

    let cookie = create_refresh_cookie(&auth_res.tokens.refresh_token, false);
    let updated_jar = jar.add(cookie);

    let (status, resp) = BaseResponse::success(auth_res, "Token refreshed successfully");
    Ok((status, updated_jar, resp))
}

pub async fn logout_handler(
    State(service): State<Arc<AuthService>>,
    jar: CookieJar,
    axum::extract::Json(body): axum::extract::Json<RefreshTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(token) = body
        .refresh_token
        .or_else(|| jar.get(REFRESH_COOKIE_NAME).map(|c| c.value().to_string()))
    {
        let _ = service.logout(&token).await;
    }

    let updated_jar = jar.add(clear_refresh_cookie());
    let (status, resp) = BaseResponse::ok_msg("Logout successful");
    Ok((status, updated_jar, resp))
}

pub async fn logout_all_handler(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<AuthService>>,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    service.logout_all(auth_user.id).await?;
    let updated_jar = jar.add(clear_refresh_cookie());
    let (status, resp) = BaseResponse::ok_msg("Logged out from all devices successfully");
    Ok((status, updated_jar, resp))
}

pub async fn change_password_handler(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<AuthService>>,
    jar: CookieJar,
    ValidatedJson(req): ValidatedJson<ChangePasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    service.change_password(auth_user.id, req).await?;
    let updated_jar = jar.add(clear_refresh_cookie());
    let (status, resp) =
        BaseResponse::ok_msg("Password updated successfully. Please log in again.");
    Ok((status, updated_jar, resp))
}

pub async fn get_sessions_handler(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<AuthService>>,
) -> Result<impl IntoResponse, AppError> {
    let sessions = service.get_sessions(auth_user.id).await?;
    Ok(BaseResponse::success(
        sessions,
        "Active sessions retrieved successfully",
    ))
}

pub async fn get_current_user_handler(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<AuthService>>,
) -> Result<impl IntoResponse, AppError> {
    let user = service.get_current_user(auth_user.id).await?;
    Ok(BaseResponse::success(
        user,
        "Current user retrieved successfully",
    ))
}

pub fn auth_router(service: Arc<AuthService>) -> Router {
    Router::new()
        .route("/register", post(register_handler))
        .route("/login", post(login_handler))
        .route("/refresh", post(refresh_handler))
        .route("/logout", post(logout_handler))
        .route("/logout-all", post(logout_all_handler))
        .route("/change-password", post(change_password_handler))
        .route("/sessions", get(get_sessions_handler))
        .route("/me", get(get_current_user_handler))
        .with_state(service)
}
