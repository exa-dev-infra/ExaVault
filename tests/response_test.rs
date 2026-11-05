use serde_json::json;
use exavault::{
    constants::codes::CODE_SUCCESS,
    pkg::response::{BaseResponse, PaginatedResponse},
};

#[test]
fn test_base_response_success_serialization() {
    let payload = json!({ "id": 1, "name": "Antigravity" });
    let (status, body) = BaseResponse::success(payload.clone(), "Operation completed");

    assert_eq!(status, axum::http::StatusCode::OK);

    let serialized = serde_json::to_value(&body.0).expect("failed to serialize BaseResponse");

    assert_eq!(serialized["success"], true);
    assert_eq!(serialized["code"], CODE_SUCCESS);
    assert_eq!(serialized["message"], "Operation completed");
    assert_eq!(serialized["data"]["name"], "Antigravity");
    assert!(serialized["timestamp"].is_string());
}

#[test]
fn test_base_response_ok_msg_serialization() {
    let (status, body) = BaseResponse::ok_msg("Deleted successfully");

    assert_eq!(status, axum::http::StatusCode::OK);

    let serialized = serde_json::to_value(&body.0).expect("failed to serialize BaseResponse");

    assert_eq!(serialized["success"], true);
    assert_eq!(serialized["code"], CODE_SUCCESS);
    assert_eq!(serialized["message"], "Deleted successfully");
    assert!(serialized.get("data").is_none());
}

#[test]
fn test_paginated_response_calculation() {
    let items = vec!["item1", "item2", "item3"];
    let (status, body) = PaginatedResponse::new(items, 1, 10, 25, "List items");

    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body.0.meta.page, 1);
    assert_eq!(body.0.meta.limit, 10);
    assert_eq!(body.0.meta.total, 25);
    assert_eq!(body.0.meta.total_pages, 3);
    assert_eq!(body.0.data.len(), 3);
}
