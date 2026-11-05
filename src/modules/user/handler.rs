use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{delete, get},
    Router,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    modules::user::{
        dto::{CreateUserRequest, ListUsersQuery, UpdateProfileRequest, UpdateUserRequest},
        service::UserService,
    },
    pkg::{
        auth::jwt::{AdminUser, AuthenticatedUser},
        errors::AppError,
        response::{BaseResponse, PaginatedResponse},
        validation::{ValidatedJson, ValidatedQuery},
    },
};

// ---------------------------------------------------------------------------
// Self-Service Profile Endpoints (Any Authenticated User)
// ---------------------------------------------------------------------------

pub async fn get_profile_handler(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<UserService>>,
) -> Result<impl IntoResponse, AppError> {
    let user = service.get_profile(auth_user.id).await?;
    Ok(BaseResponse::success(
        user,
        "User profile retrieved successfully",
    ))
}

pub async fn update_profile_handler(
    auth_user: AuthenticatedUser,
    State(service): State<Arc<UserService>>,
    ValidatedJson(req): ValidatedJson<UpdateProfileRequest>,
) -> Result<impl IntoResponse, AppError> {
    let updated = service.update_profile(auth_user.id, req).await?;
    Ok(BaseResponse::success(
        updated,
        "Profile updated successfully",
    ))
}

// ---------------------------------------------------------------------------
// Administrative CRUD Endpoints (Strictly Admin Role Required)
// ---------------------------------------------------------------------------

pub async fn list_users_handler(
    _admin: AdminUser,
    State(service): State<Arc<UserService>>,
    ValidatedQuery(query): ValidatedQuery<ListUsersQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (users, page, limit, total) = service.list_users(query).await?;
    Ok(PaginatedResponse::new(
        users,
        page,
        limit,
        total,
        "Users retrieved successfully",
    ))
}

pub async fn get_user_handler(
    _admin: AdminUser,
    State(service): State<Arc<UserService>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let user = service.get_user_by_id(id).await?;
    Ok(BaseResponse::success(user, "User retrieved successfully"))
}

pub async fn create_user_handler(
    _admin: AdminUser,
    State(service): State<Arc<UserService>>,
    ValidatedJson(req): ValidatedJson<CreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user = service.create_user(req).await?;
    let (status, body) = BaseResponse::created(user, "User created successfully");
    Ok((status, body))
}

pub async fn update_user_handler(
    _admin: AdminUser,
    State(service): State<Arc<UserService>>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user = service.update_user(id, req).await?;
    Ok(BaseResponse::success(user, "User updated successfully"))
}

pub async fn delete_user_handler(
    _admin: AdminUser,
    State(service): State<Arc<UserService>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    service.delete_user(id).await?;
    Ok(BaseResponse::ok_msg("User deleted successfully"))
}

pub async fn hard_delete_user_handler(
    _admin: AdminUser,
    State(service): State<Arc<UserService>>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    service.hard_delete_user(id).await?;
    Ok(BaseResponse::ok_msg("User permanently deleted"))
}

pub fn user_router(service: Arc<UserService>) -> Router {
    Router::new()
        .route(
            "/profile",
            get(get_profile_handler).put(update_profile_handler),
        )
        .route("/", get(list_users_handler).post(create_user_handler))
        .route(
            "/:id",
            get(get_user_handler)
                .put(update_user_handler)
                .delete(delete_user_handler),
        )
        .route("/:id/hard", delete(hard_delete_user_handler))
        .route("/admin/:id", delete(hard_delete_user_handler))
        .with_state(service)
}
