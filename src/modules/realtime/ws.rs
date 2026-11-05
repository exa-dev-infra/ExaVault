use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;

use crate::pkg::realtime::RealtimeHub;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(hub): State<Arc<RealtimeHub>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, hub))
}

async fn handle_socket(socket: WebSocket, hub: Arc<RealtimeHub>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = hub.subscribe();

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let hub_clone = hub.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                hub_clone.broadcast(text).await;
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
