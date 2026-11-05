use axum::{http::StatusCode, Json};
use chrono::Utc;
use serde::Serialize;

use crate::constants::codes::{CODE_CREATED, CODE_SUCCESS};

#[derive(Debug, Serialize, Clone)]
pub struct BaseResponse<T> {
    pub message: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub code: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PaginationMeta {
    pub page: u64,
    pub limit: u64,
    pub total: u64,
    pub total_pages: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct PaginatedResponse<T> {
    pub message: String,
    pub success: bool,
    pub data: Vec<T>,
    pub meta: PaginationMeta,
    pub code: String,
    pub timestamp: String,
}

impl<T> BaseResponse<T> {
    pub fn success(data: T, message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::OK,
            Json(Self {
                message: message.into(),
                success: true,
                data: Some(data),
                code: CODE_SUCCESS.to_string(),
                timestamp: Utc::now().to_rfc3339(),
            }),
        )
    }

    pub fn created(data: T, message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::CREATED,
            Json(Self {
                message: message.into(),
                success: true,
                data: Some(data),
                code: CODE_CREATED.to_string(),
                timestamp: Utc::now().to_rfc3339(),
            }),
        )
    }
}

impl BaseResponse<()> {
    pub fn ok_msg(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::OK,
            Json(Self {
                message: message.into(),
                success: true,
                data: None,
                code: CODE_SUCCESS.to_string(),
                timestamp: Utc::now().to_rfc3339(),
            }),
        )
    }
}

impl<T: Serialize> PaginatedResponse<T> {
    pub fn new(
        data: Vec<T>,
        page: u64,
        limit: u64,
        total: u64,
        message: impl Into<String>,
    ) -> (StatusCode, Json<Self>) {
        let total_pages = if limit == 0 { 0 } else { total.div_ceil(limit) };

        (
            StatusCode::OK,
            Json(Self {
                message: message.into(),
                success: true,
                data,
                meta: PaginationMeta {
                    page,
                    limit,
                    total,
                    total_pages,
                },
                code: CODE_SUCCESS.to_string(),
                timestamp: Utc::now().to_rfc3339(),
            }),
        )
    }
}
