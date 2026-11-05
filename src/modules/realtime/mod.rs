pub mod sse;
pub mod ws;

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{pkg::realtime::RealtimeHub, pkg::response::BaseResponse};

#[derive(Debug, Deserialize)]
pub struct BroadcastRequest {
    pub message: String,
}

pub async fn broadcast_handler(
    State(hub): State<Arc<RealtimeHub>>,
    Json(payload): Json<BroadcastRequest>,
) -> impl IntoResponse {
    hub.broadcast(&payload.message).await;
    BaseResponse::ok_msg("Message broadcasted successfully")
}

pub fn realtime_router(hub: Arc<RealtimeHub>) -> Router {
    Router::new()
        .route("/ws", get(ws::ws_handler))
        .route("/sse", get(sse::sse_handler))
        .route("/broadcast", post(broadcast_handler))
        .with_state(hub)
}
