use axum::{
    async_trait,
    body::Bytes,
    extract::{multipart::Multipart, FromRequest, FromRequestParts, Path, Query, Request},
    http::request::Parts,
    Json,
};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use validator::Validate;

use crate::{constants::codes::CODE_BAD_REQUEST, pkg::errors::AppError};

// ---------------------------------------------------------------------------
// 1. JSON Body Validation: `ValidatedJson<T>`
// ---------------------------------------------------------------------------

/// Extractor that deserializes a JSON body and validates it using `validator::Validate`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedJson<T>(pub T);

impl<T> std::ops::Deref for ValidatedJson<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(data) = match Json::<T>::from_request(req, state).await {
            Ok(json) => json,
            Err(rejection) => {
                return Err(AppError::bad_request(
                    format!("Malformed JSON payload: {}", rejection),
                    CODE_BAD_REQUEST,
                ));
            }
        };

        if let Err(errors) = data.validate() {
            let details = serde_json::to_value(&errors).ok();
            return Err(AppError::validation("Validation failed", details));
        }

        Ok(ValidatedJson(data))
    }
}

// ---------------------------------------------------------------------------
// 2. Query String Validation: `ValidatedQuery<T>`
// ---------------------------------------------------------------------------

/// Extractor that parses URL query parameters and validates them using `validator::Validate`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedQuery<T>(pub T);

impl<T> std::ops::Deref for ValidatedQuery<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(data) = match Query::<T>::from_request_parts(parts, state).await {
            Ok(query) => query,
            Err(rejection) => {
                return Err(AppError::bad_request(
                    format!("Malformed query parameters: {}", rejection),
                    CODE_BAD_REQUEST,
                ));
            }
        };

        if let Err(errors) = data.validate() {
            let details = serde_json::to_value(&errors).ok();
            return Err(AppError::validation("Query validation failed", details));
        }

        Ok(ValidatedQuery(data))
    }
}

// ---------------------------------------------------------------------------
// 3. Route / Path Parameters Validation: `ValidatedPath<T>`
// ---------------------------------------------------------------------------

/// Extractor that parses route/path parameters and validates them using `validator::Validate`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedPath<T>(pub T);

impl<T> std::ops::Deref for ValidatedPath<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<T, S> FromRequestParts<S> for ValidatedPath<T>
where
    T: DeserializeOwned + Validate + Send,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(data) = match Path::<T>::from_request_parts(parts, state).await {
            Ok(path) => path,
            Err(rejection) => {
                return Err(AppError::bad_request(
                    format!("Malformed path parameters: {}", rejection),
                    CODE_BAD_REQUEST,
                ));
            }
        };

        if let Err(errors) = data.validate() {
            let details = serde_json::to_value(&errors).ok();
            return Err(AppError::validation("Path validation failed", details));
        }

        Ok(ValidatedPath(data))
    }
}

// ---------------------------------------------------------------------------
// 4. URL-Encoded Form Validation: `ValidatedForm<T>`
// ---------------------------------------------------------------------------

/// Extractor that parses form data (`application/x-www-form-urlencoded`) and validates it using `validator::Validate`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedForm<T>(pub T);

impl<T> std::ops::Deref for ValidatedForm<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<T, S> FromRequest<S> for ValidatedForm<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Form(data) =
            match axum::extract::Form::<T>::from_request(req, state).await {
                Ok(form) => form,
                Err(rejection) => {
                    return Err(AppError::bad_request(
                        format!("Malformed form payload: {}", rejection),
                        CODE_BAD_REQUEST,
                    ));
                }
            };

        if let Err(errors) = data.validate() {
            let details = serde_json::to_value(&errors).ok();
            return Err(AppError::validation("Form validation failed", details));
        }

        Ok(ValidatedForm(data))
    }
}

// ---------------------------------------------------------------------------
// 5. Multipart Form Data & File Uploads: `ParsedMultipart` & `UploadedFile`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UploadedFile {
    pub field_name: String,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub data: Bytes,
}

impl UploadedFile {
    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn validate_max_size(&self, max_bytes: usize) -> Result<(), AppError> {
        if self.data.len() > max_bytes {
            return Err(AppError::bad_request(
                format!(
                    "File '{}' exceeds maximum allowed size of {} bytes",
                    self.file_name.as_deref().unwrap_or("unnamed"),
                    max_bytes
                ),
                CODE_BAD_REQUEST,
            ));
        }
        Ok(())
    }

    pub fn validate_allowed_types(&self, allowed: &[&str]) -> Result<(), AppError> {
        let ct = self
            .content_type
            .as_deref()
            .unwrap_or("application/octet-stream");
        if !allowed.iter().any(|&item| item.eq_ignore_ascii_case(ct)) {
            return Err(AppError::bad_request(
                format!(
                    "File '{}' has unsupported content-type '{}'. Allowed: {:?}",
                    self.file_name.as_deref().unwrap_or("unnamed"),
                    ct,
                    allowed
                ),
                CODE_BAD_REQUEST,
            ));
        }
        Ok(())
    }
}

/// Extractor that parses `multipart/form-data` into structured text fields and uploaded files.
#[derive(Debug, Clone, Default)]
pub struct ParsedMultipart {
    pub fields: HashMap<String, String>,
    pub files: HashMap<String, Vec<UploadedFile>>,
}

impl ParsedMultipart {
    pub fn get_field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(|s| s.as_str())
    }

    pub fn get_file(&self, key: &str) -> Option<&UploadedFile> {
        self.files.get(key).and_then(|list| list.first())
    }

    pub fn get_files(&self, key: &str) -> Option<&[UploadedFile]> {
        self.files.get(key).map(|v| v.as_slice())
    }

    /// Binds and validates text form fields into a target DTO struct using `serde` and `validator`.
    pub fn bind<T: DeserializeOwned + Validate>(&self) -> Result<T, AppError> {
        let json_value = serde_json::to_value(&self.fields).map_err(|e| {
            AppError::bad_request(
                format!("Failed to parse multipart fields: {}", e),
                CODE_BAD_REQUEST,
            )
        })?;

        let dto: T = serde_json::from_value(json_value).map_err(|e| {
            AppError::bad_request(
                format!("Malformed multipart form fields: {}", e),
                CODE_BAD_REQUEST,
            )
        })?;

        if let Err(errors) = dto.validate() {
            let details = serde_json::to_value(&errors).ok();
            return Err(AppError::validation(
                "Multipart form validation failed",
                details,
            ));
        }

        Ok(dto)
    }
}

#[async_trait]
impl<S> FromRequest<S> for ParsedMultipart
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let mut multipart = match Multipart::from_request(req, state).await {
            Ok(mp) => mp,
            Err(rejection) => {
                return Err(AppError::bad_request(
                    format!("Invalid multipart payload: {}", rejection),
                    CODE_BAD_REQUEST,
                ));
            }
        };

        let mut fields = HashMap::new();
        let mut files = HashMap::new();

        while let Some(field) = multipart.next_field().await.map_err(|e| {
            AppError::bad_request(
                format!("Error reading multipart field: {}", e),
                CODE_BAD_REQUEST,
            )
        })? {
            let field_name = field.name().unwrap_or("").to_string();
            let file_name = field.file_name().map(|s| s.to_string());
            let content_type = field.content_type().map(|s| s.to_string());

            if file_name.is_some() {
                let data = field.bytes().await.map_err(|e| {
                    AppError::bad_request(
                        format!("Error reading upload bytes: {}", e),
                        CODE_BAD_REQUEST,
                    )
                })?;

                let uploaded_file = UploadedFile {
                    field_name: field_name.clone(),
                    file_name,
                    content_type,
                    data,
                };

                files
                    .entry(field_name)
                    .or_insert_with(Vec::new)
                    .push(uploaded_file);
            } else {
                let text = field.text().await.map_err(|e| {
                    AppError::bad_request(
                        format!("Error reading text field: {}", e),
                        CODE_BAD_REQUEST,
                    )
                })?;
                fields.insert(field_name, text);
            }
        }

        Ok(ParsedMultipart { fields, files })
    }
}
