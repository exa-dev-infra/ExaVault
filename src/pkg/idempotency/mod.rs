use axum::{
    body::{to_bytes, Body, Bytes},
    extract::Request,
    http::{header::HeaderName, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

use crate::{constants::codes::CODE_IDEMPOTENCY_IN_FLIGHT, pkg::errors::ErrorResponse};

pub const HEADER_IDEMPOTENCY_KEY: &str = "idempotency-key";
pub const HEADER_ALT_IDEMPOTENCY_KEY: &str = "x-idempotency-key";
pub const HEADER_IDEMPOTENT_REPLAYED: &str = "idempotent-replayed";

#[derive(Clone, Debug)]
pub enum IdempotencyEntry {
    InProgress {
        locked_at: Instant,
        lock_timeout: Duration,
    },
    Completed {
        status: StatusCode,
        headers: Vec<(String, String)>,
        body: Bytes,
        expires_at: Instant,
    },
}

#[derive(Clone)]
pub struct IdempotencyStore {
    entries: Arc<RwLock<HashMap<String, IdempotencyEntry>>>,
    lifetime: Duration,
    lock_timeout: Duration,
}

impl Default for IdempotencyStore {
    fn default() -> Self {
        Self::new(Duration::from_secs(86400), Duration::from_secs(30))
    }
}

impl IdempotencyStore {
    pub fn new(lifetime: Duration, lock_timeout: Duration) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            lifetime,
            lock_timeout,
        }
    }

    pub async fn get(&self, key: &str) -> Option<IdempotencyEntry> {
        let mut store = self.entries.write().await;
        if let Some(entry) = store.get(key) {
            match entry {
                IdempotencyEntry::InProgress {
                    locked_at,
                    lock_timeout,
                } => {
                    if locked_at.elapsed() > *lock_timeout {
                        // Lock expired, evict so client can retry
                        store.remove(key);
                        None
                    } else {
                        Some(entry.clone())
                    }
                }
                IdempotencyEntry::Completed { expires_at, .. } => {
                    if Instant::now() > *expires_at {
                        // Expired entry
                        store.remove(key);
                        None
                    } else {
                        Some(entry.clone())
                    }
                }
            }
        } else {
            None
        }
    }

    pub async fn try_lock(&self, key: &str) -> bool {
        let mut store = self.entries.write().await;
        if let Some(entry) = store.get(key) {
            match entry {
                IdempotencyEntry::InProgress {
                    locked_at,
                    lock_timeout,
                } => {
                    if locked_at.elapsed() <= *lock_timeout {
                        return false;
                    }
                }
                IdempotencyEntry::Completed { expires_at, .. } => {
                    if Instant::now() <= *expires_at {
                        return false;
                    }
                }
            }
        }

        store.insert(
            key.to_string(),
            IdempotencyEntry::InProgress {
                locked_at: Instant::now(),
                lock_timeout: self.lock_timeout,
            },
        );
        true
    }

    pub async fn save(
        &self,
        key: &str,
        status: StatusCode,
        headers: Vec<(String, String)>,
        body: Bytes,
    ) {
        let mut store = self.entries.write().await;
        store.insert(
            key.to_string(),
            IdempotencyEntry::Completed {
                status,
                headers,
                body,
                expires_at: Instant::now() + self.lifetime,
            },
        );
    }

    pub async fn unlock(&self, key: &str) {
        let mut store = self.entries.write().await;
        store.remove(key);
    }
}

/// Idempotency middleware for Axum that intercepts mutation requests (POST, PUT, PATCH).
pub async fn idempotency_layer(store: Arc<IdempotencyStore>, req: Request, next: Next) -> Response {
    let method = req.method().clone();
    if method != Method::POST && method != Method::PUT && method != Method::PATCH {
        return next.run(req).await;
    }

    let key_header = req
        .headers()
        .get(HEADER_IDEMPOTENCY_KEY)
        .or_else(|| req.headers().get(HEADER_ALT_IDEMPOTENCY_KEY))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string());

    let idempotency_key = match key_header {
        Some(k) if !k.is_empty() => k,
        _ => return next.run(req).await,
    };

    // Check if key is already stored or in-flight
    if let Some(entry) = store.get(&idempotency_key).await {
        match entry {
            IdempotencyEntry::InProgress { .. } => {
                let err = ErrorResponse {
                    message: "A request with this idempotency key is currently in-flight"
                        .to_string(),
                    success: false,
                    code: CODE_IDEMPOTENCY_IN_FLIGHT.to_string(),
                    errors: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                return (StatusCode::CONFLICT, axum::Json(err)).into_response();
            }
            IdempotencyEntry::Completed {
                status,
                headers,
                body,
                ..
            } => {
                let mut response = Response::builder().status(status);
                for (name, val) in headers {
                    if let (Ok(hname), Ok(hval)) = (
                        HeaderName::from_bytes(name.as_bytes()),
                        HeaderValue::from_str(&val),
                    ) {
                        response = response.header(hname, hval);
                    }
                }
                response = response.header(HEADER_IDEMPOTENT_REPLAYED, "true");
                return response
                    .body(Body::from(body))
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        }
    }

    // Attempt to acquire lock
    if !store.try_lock(&idempotency_key).await {
        let err = ErrorResponse {
            message: "A request with this idempotency key is currently in-flight".to_string(),
            success: false,
            code: CODE_IDEMPOTENCY_IN_FLIGHT.to_string(),
            errors: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        return (StatusCode::CONFLICT, axum::Json(err)).into_response();
    }

    // Execute request pipeline
    let resp = next.run(req).await;

    // Buffer response body so we can cache it and still return it to client
    let (parts, body) = resp.into_parts();
    let bytes = match to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            store.unlock(&idempotency_key).await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Extract saveable headers (content-type, etc.)
    let mut save_headers = Vec::new();
    for (k, v) in &parts.headers {
        if let Ok(v_str) = v.to_str() {
            save_headers.push((k.as_str().to_string(), v_str.to_string()));
        }
    }

    // Cache completed response
    store
        .save(&idempotency_key, parts.status, save_headers, bytes.clone())
        .await;

    Response::from_parts(parts, Body::from(bytes))
}
