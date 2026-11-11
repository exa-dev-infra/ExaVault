pub mod config;
pub mod constants;
pub mod database;
pub mod modules;
pub mod pkg;

use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tower_http::cors::CorsLayer;

use crate::{
    config::AppConfig,
    modules::{
        auth::{handler::auth_router, service::AuthService},
        realtime::realtime_router,
        user::{handler::user_router, service::UserService},
        vault::{
            handler::{export_secrets_handler, VaultContext},
            repository::VaultRepository,
            service::VaultService,
            vault_router,
        },
        web::web_router,
    },
    pkg::{
        auth::jwt::{AuthenticatedUser, OptionalAuthUser},
        idempotency::{idempotency_layer, IdempotencyStore},
        logging::middleware::create_trace_layer,
        realtime::RealtimeHub,
        response::BaseResponse,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub pool: PgPool,
    pub hub: Arc<RealtimeHub>,
    pub vault_service: Arc<VaultService>,
}

pub async fn root_handler() -> impl IntoResponse {
    BaseResponse::success(
        json!({
            "status": "running",
            "app": "ExaVault",
            "time": Utc::now().to_rfc3339()
        }),
        "Welcome to ExaVault API",
    )
}

pub async fn health_check_handler(State(state): State<AppState>) -> Json<Value> {
    let mut is_healthy = true;

    // 1. Database check
    let db_status = match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => "connected",
        Err(_) => {
            is_healthy = false;
            "error"
        }
    };

    // 2. Redis check
    let redis_status = if state.config.redis_enabled {
        match crate::pkg::cache::RedisCache::new(&state.config.redis_url).await {
            Ok(cache) => match cache.health_check().await {
                Ok(_) => "connected",
                Err(_) => {
                    is_healthy = false;
                    "error"
                }
            },
            Err(_) => {
                is_healthy = false;
                "error"
            }
        }
    } else {
        "disabled"
    };

    // 3. RabbitMQ check
    let rabbitmq_status = if state.config.rabbitmq_enabled {
        match crate::pkg::queue::RabbitMQBroker::new(&state.config.rabbitmq_url).await {
            Ok(broker) => match broker.health_check().await {
                Ok(_) => "connected",
                Err(_) => {
                    is_healthy = false;
                    "error"
                }
            },
            Err(_) => {
                is_healthy = false;
                "error"
            }
        }
    } else {
        "disabled"
    };

    // 4. Realtime status
    let realtime_status = if state.config.realtime_ws_enabled || state.config.realtime_sse_enabled {
        "connected"
    } else {
        "disabled"
    };

    Json(json!({
        "status": if is_healthy { "healthy" } else { "degraded" },
        "app": state.config.app_name,
        "environment": state.config.environment,
        "database": db_status,
        "redis": redis_status,
        "rabbitmq": rabbitmq_status,
        "realtime": realtime_status,
        "features": {
            "redis": state.config.redis_enabled,
            "rabbitmq": state.config.rabbitmq_enabled,
            "idempotency": state.config.idempotency_enabled,
            "realtime_ws": state.config.realtime_ws_enabled,
            "realtime_sse": state.config.realtime_sse_enabled,
            "realtime_bridge": state.config.realtime_bridge,
            "otel_enabled": state.config.otel_enabled,
        },
        "timestamp": Utc::now().to_rfc3339()
    }))
}

pub async fn protected_data_handler(auth_user: AuthenticatedUser) -> Json<Value> {
    Json(json!({
        "message": "This is protected data",
        "user_id": auth_user.id,
        "email": auth_user.email,
        "role": auth_user.role,
    }))
}

pub async fn public_or_private_handler(optional_user: OptionalAuthUser) -> Json<Value> {
    if let Some(user) = optional_user.0 {
        Json(json!({
            "message": "Private data",
            "authenticated": true,
            "user_id": user.id,
            "email": user.email,
        }))
    } else {
        Json(json!({
            "message": "Public data",
            "authenticated": false,
        }))
    }
}

pub fn build_app(config: AppConfig, pool: PgPool, hub: Arc<RealtimeHub>) -> Router {
    // 1. Initialize ExaVault Repository & Service
    let vault_repo = Arc::new(VaultRepository::new(pool.clone()));
    let vault_service = Arc::new(VaultService::new(
        vault_repo.clone(),
        config.master_key_bytes(),
    ));

    let vault_ctx = VaultContext {
        service: vault_service.clone(),
        admin_username: config.admin_username.clone(),
        admin_password: Arc::new(tokio::sync::RwLock::new(config.admin_password.clone())),
        jwt_secret: config.jwt_secret.clone(),
    };

    let state = AppState {
        config: config.clone(),
        pool: pool.clone(),
        hub: hub.clone(),
        vault_service: vault_service.clone(),
    };

    let user_service = Arc::new(UserService::with_default_repo(pool.clone()));
    let auth_service = Arc::new(AuthService::with_default_repos(pool, config.clone()));

    // 2. Build API routes
    let mut api_routes = Router::new()
        .nest("/auth", auth_router(auth_service))
        .nest("/users", user_router(user_service))
        .nest("/realtime", realtime_router(hub))
        .route("/export", get(export_secrets_handler).with_state(vault_ctx.clone()))
        .route("/protected/data", get(protected_data_handler))
        .route("/public-or-private", get(public_or_private_handler));

    // Optional Idempotency Middleware for mutation requests (POST, PUT, PATCH)
    if config.idempotency_enabled {
        let idempotency_store = Arc::new(IdempotencyStore::new(
            Duration::from_secs(config.idempotency_lifetime_hours * 3600),
            Duration::from_secs(config.idempotency_lock_timeout_seconds),
        ));
        api_routes = api_routes.layer(axum::middleware::from_fn(move |req, next| {
            idempotency_layer(idempotency_store.clone(), req, next)
        }));
    }

    let health_router = Router::new()
        .route("/health", get(health_check_handler))
        .with_state(state);

    // 3. ExaVault Dashboard API & Web UI
    let vault_api_router = vault_router(vault_ctx.clone());
    let web_ui_router = web_router(vault_ctx);

    Router::new()
        .merge(web_ui_router)
        .merge(health_router)
        .nest("/api", vault_api_router)
        .nest("/api/v1", api_routes)
        .layer(create_trace_layer::<axum::body::Body>())
        .layer(CorsLayer::permissive())
}
