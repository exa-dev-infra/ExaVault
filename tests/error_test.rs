use axum::{http::StatusCode, response::IntoResponse};
use exavault::{
    constants::{codes::*, database::*},
    pkg::errors::{parse_database_error, AppError},
};

#[test]
fn test_semantic_app_errors() {
    let not_found = AppError::user_not_found();
    let resp = not_found.into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let conflict = AppError::email_already_exists();
    let resp_conflict = conflict.into_response();
    assert_eq!(resp_conflict.status(), StatusCode::CONFLICT);

    let unauth = AppError::invalid_credentials();
    let resp_unauth = unauth.into_response();
    assert_eq!(resp_unauth.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_generic_database_error_mapping() {
    let sqlx_err = sqlx::Error::RowNotFound;
    let (status, code, msg) = parse_database_error(&sqlx_err);

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(code, CODE_DATABASE_ERROR);
    assert_eq!(msg, "Internal database error occurred");
}

#[test]
fn test_constraint_lookup_definitions() {
    // Unique constraint explicit mapping
    let email_mapping = lookup_unique_constraint("users_email_key");
    assert!(email_mapping.is_some());
    let (field, code) = email_mapping.unwrap();
    assert_eq!(field, "email");
    assert_eq!(code, CODE_EMAIL_ALREADY_EXISTS);

    // Foreign key constraint explicit mapping
    let fk_mapping = lookup_foreign_key_constraint("refresh_tokens_user_id_fkey");
    assert!(fk_mapping.is_some());
    assert_eq!(fk_mapping.unwrap(), "user");

    // Unregistered constraint returns None (ready for auto-deduction)
    assert!(lookup_unique_constraint("unknown_table_custom_field_key").is_none());
    assert!(lookup_foreign_key_constraint("unknown_table_custom_id_fkey").is_none());
}


