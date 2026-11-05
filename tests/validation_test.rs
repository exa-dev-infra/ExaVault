use axum::{
    body::Bytes,
    extract::{FromRequest, FromRequestParts, Request},
    http::header::CONTENT_TYPE,
};
use exavault::{
    modules::{
        auth::dto::{ChangePasswordRequest, LoginRequest, RegisterRequest},
        user::dto::{
            CreateUserRequest, ListUsersQuery, UpdateProfileRequest, UpdateUserRequest,
            UserPathParams,
        },
    },
    pkg::validation::{
        ParsedMultipart, UploadedFile, ValidatedForm, ValidatedJson, ValidatedPath, ValidatedQuery,
    },
};
use uuid::Uuid;
use validator::Validate;

// ---------------------------------------------------------------------------
// Struct DTO Level Tests
// ---------------------------------------------------------------------------

#[test]
fn test_register_request_validation_success() {
    let req = RegisterRequest {
        name: "Alice Developer".to_string(),
        email: "alice@example.com".to_string(),
        password: "securepassword123".to_string(),
    };

    assert!(req.validate().is_ok());
}

#[test]
fn test_register_request_validation_failures() {
    // Invalid email
    let req_invalid_email = RegisterRequest {
        name: "Alice".to_string(),
        email: "not-an-email".to_string(),
        password: "securepassword123".to_string(),
    };
    assert!(req_invalid_email.validate().is_err());

    // Password too short (< 8 chars)
    let req_short_password = RegisterRequest {
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
        password: "short".to_string(),
    };
    assert!(req_short_password.validate().is_err());

    // Name too short (< 2 chars)
    let req_short_name = RegisterRequest {
        name: "A".to_string(),
        email: "alice@example.com".to_string(),
        password: "securepassword123".to_string(),
    };
    assert!(req_short_name.validate().is_err());
}

#[test]
fn test_login_request_validation() {
    let valid_login = LoginRequest {
        email: "user@example.com".to_string(),
        password: "password123".to_string(),
    };
    assert!(valid_login.validate().is_ok());

    let empty_pwd = LoginRequest {
        email: "user@example.com".to_string(),
        password: "".to_string(),
    };
    assert!(empty_pwd.validate().is_err());
}

#[test]
fn test_update_user_validation() {
    let valid = UpdateUserRequest {
        name: Some("Bob Builder".to_string()),
        email: Some("bob@example.com".to_string()),
        phone: Some("081234567890".to_string()),
        role: Some("admin".to_string()),
        is_active: Some(true),
    };
    assert!(valid.validate().is_ok());

    let invalid = UpdateUserRequest {
        name: Some("B".to_string()),
        email: None,
        phone: None,
        role: None,
        is_active: None,
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn test_create_user_validation() {
    let valid = CreateUserRequest {
        name: "Charlie Admin".to_string(),
        email: "charlie@example.com".to_string(),
        phone: Some("081234567899".to_string()),
        role: Some("admin".to_string()),
        password: Some("adminpassword123".to_string()),
    };
    assert!(valid.validate().is_ok());

    let invalid_phone = CreateUserRequest {
        name: "Charlie".to_string(),
        email: "charlie@example.com".to_string(),
        phone: Some("123".to_string()), // too short
        role: None,
        password: None,
    };
    assert!(invalid_phone.validate().is_err());
}

#[test]
fn test_update_profile_validation() {
    let valid = UpdateProfileRequest {
        name: Some("Dave".to_string()),
        phone: Some("081234567888".to_string()),
    };
    assert!(valid.validate().is_ok());
}

#[test]
fn test_change_password_validation() {
    let valid = ChangePasswordRequest {
        current_password: "oldpassword123".to_string(),
        new_password: "newsecurepassword456".to_string(),
    };
    assert!(valid.validate().is_ok());

    let short = ChangePasswordRequest {
        current_password: "oldpassword123".to_string(),
        new_password: "short".to_string(),
    };
    assert!(short.validate().is_err());
}

#[test]
fn test_query_validation_ranges() {
    let valid_query = ListUsersQuery {
        page: Some(1),
        limit: Some(50),
        search: None,
        role: None,
        is_active: None,
    };
    assert!(valid_query.validate().is_ok());

    let invalid_page = ListUsersQuery {
        page: Some(0), // min is 1
        limit: Some(10),
        search: None,
        role: None,
        is_active: None,
    };
    assert!(invalid_page.validate().is_err());

    let invalid_limit = ListUsersQuery {
        page: Some(1),
        limit: Some(500), // max is 100
        search: None,
        role: None,
        is_active: None,
    };
    assert!(invalid_limit.validate().is_err());
}

// ---------------------------------------------------------------------------
// Extractor Level Tests (Body, Query, Path, Form, Multipart)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_validated_json_extractor_success_and_failure() {
    // 1. Valid JSON body
    let req = Request::builder()
        .header(CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            r#"{"name":"Eve Online","email":"eve@example.com","password":"mypassword123"}"#,
        ))
        .unwrap();

    let res = ValidatedJson::<RegisterRequest>::from_request(req, &()).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap().name, "Eve Online");

    // 2. Invalid validation (password too short)
    let req_invalid = Request::builder()
        .header(CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            r#"{"name":"Eve","email":"eve@example.com","password":"short"}"#,
        ))
        .unwrap();

    let res_invalid = ValidatedJson::<RegisterRequest>::from_request(req_invalid, &()).await;
    assert!(res_invalid.is_err());

    // 3. Malformed JSON syntax
    let req_malformed = Request::builder()
        .header(CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from("{malformed:json"))
        .unwrap();

    let res_malformed = ValidatedJson::<RegisterRequest>::from_request(req_malformed, &()).await;
    assert!(res_malformed.is_err());
}

#[tokio::test]
async fn test_validated_query_extractor() {
    // 1. Valid Query string
    let req = Request::builder()
        .uri("/api/v1/users?page=2&limit=25")
        .body(())
        .unwrap();
    let (mut parts, _) = req.into_parts();

    let res = ValidatedQuery::<ListUsersQuery>::from_request_parts(&mut parts, &()).await;
    assert!(res.is_ok());
    let query = res.unwrap();
    assert_eq!(query.page, Some(2));
    assert_eq!(query.limit, Some(25));

    // 2. Invalid Query range (limit > 100)
    let req_invalid = Request::builder()
        .uri("/api/v1/users?page=1&limit=200")
        .body(())
        .unwrap();
    let (mut parts_inv, _) = req_invalid.into_parts();

    let res_invalid =
        ValidatedQuery::<ListUsersQuery>::from_request_parts(&mut parts_inv, &()).await;
    assert!(res_invalid.is_err());
}

#[tokio::test]
async fn test_validated_form_extractor() {
    let req = Request::builder()
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(axum::body::Body::from(
            "name=Frank&email=frank%40example.com&password=securepassword123",
        ))
        .unwrap();

    let res = ValidatedForm::<RegisterRequest>::from_request(req, &()).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap().email, "frank@example.com");
}

#[tokio::test]
async fn test_validated_path_extractor() {
    let valid_path = UserPathParams { id: Uuid::new_v4() };
    assert!(valid_path.validate().is_ok());

    let req = Request::builder().uri("/users/invalid").body(()).unwrap();
    let (mut parts, _) = req.into_parts();
    let res = ValidatedPath::<UserPathParams>::from_request_parts(&mut parts, &()).await;
    assert!(res.is_err());
}

#[test]
fn test_uploaded_file_validations() {
    let file = UploadedFile {
        field_name: "avatar".to_string(),
        file_name: Some("profile.png".to_string()),
        content_type: Some("image/png".to_string()),
        data: Bytes::from_static(b"\x89PNG\r\n\x1a\nfake-image-bytes"),
    };

    assert_eq!(file.size(), 24);
    assert!(file.validate_max_size(1024).is_ok());
    assert!(file.validate_max_size(10).is_err()); // Exceeds 10 bytes

    assert!(file
        .validate_allowed_types(&["image/png", "image/jpeg"])
        .is_ok());
    assert!(file.validate_allowed_types(&["application/pdf"]).is_err());
}

#[test]
fn test_parsed_multipart_bind_dto() {
    let mut mp = ParsedMultipart::default();
    mp.fields
        .insert("name".to_string(), "Grace Hopper".to_string());
    mp.fields
        .insert("email".to_string(), "grace@navy.mil".to_string());
    mp.fields
        .insert("password".to_string(), "nanoseconds123".to_string());

    let dto: Result<RegisterRequest, _> = mp.bind();
    assert!(dto.is_ok());
    assert_eq!(dto.unwrap().name, "Grace Hopper");
}
