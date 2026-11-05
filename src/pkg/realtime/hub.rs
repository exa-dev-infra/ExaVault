use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::pkg::realtime::bridge::RealtimeBridge;

#[derive(Clone)]
pub struct RealtimeHub {
    sender: broadcast::Sender<String>,
    bridge: Arc<RwLock<Option<Arc<dyn RealtimeBridge>>>>,
}

impl RealtimeHub {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            bridge: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn set_bridge(&self, bridge: Arc<dyn RealtimeBridge>) {
        *self.bridge.write().await = Some(bridge);
    }

    pub fn sender(&self) -> broadcast::Sender<String> {
        self.sender.clone()
    }

    /// Broadcasts to local subscribers AND forwards through the distributed bridge to other instances.
    pub async fn broadcast(&self, message: impl Into<String>) {
        let msg = message.into();
        let _ = self.sender.send(msg.clone());

        if let Some(bridge) = self.bridge.read().await.as_ref() {
            let _ = bridge.publish("broadcast", &msg).await;
        }
    }

    /// Broadcasts to local clients only (called by the bridge listener).
    pub fn broadcast_local(&self, message: impl Into<String>) {
        let _ = self.sender.send(message.into());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.sender.subscribe()
    }
}

impl Default for RealtimeHub {
    fn default() -> Self {
        Self::new(1024)
    }
}
