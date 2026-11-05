pub mod dto;
pub mod handler;
pub mod repository;
pub mod service;

use axum::{
    middleware::from_fn_with_state,
    routing::{delete, get, post},
    Router,
};

use crate::modules::vault::handler::*;

pub fn vault_router(ctx: VaultContext) -> Router {
    // 1. Protected Dashboard & Vault Endpoints
    let protected_routes = Router::new()
        .route("/auth/me", get(me_handler))
        .route("/auth/logout", post(logout_handler))
        .route("/projects", get(list_projects_handler).post(create_project_handler))
        .route("/projects/:id", delete(delete_project_handler))
        .route(
            "/environments/:env_id/secrets",
            get(list_secrets_handler).put(bulk_upsert_secrets_handler),
        )
        .route("/secrets/:id", delete(delete_secret_handler))
        .route(
            "/environments/:env_id/tokens",
            get(list_tokens_handler).post(create_token_handler),
        )
        .route("/tokens/:id", delete(delete_token_handler))
        // User Management & RBAC routes
        .route("/users", get(list_users_handler).post(create_user_handler))
        .route("/users/:id", axum::routing::put(update_user_handler).delete(delete_user_handler))
        .route("/users/:id/projects", post(assign_user_project_handler))
        .route("/users/:id/projects/:project_id", delete(remove_user_project_handler))
        .layer(from_fn_with_state(ctx.clone(), require_admin_auth));

    // 2. Public Endpoints
    Router::new()
        .route("/auth/login", post(login_handler))
        .merge(protected_routes)
        .with_state(ctx)
}
