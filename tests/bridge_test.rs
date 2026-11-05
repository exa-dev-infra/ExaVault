use std::sync::Arc;
use exavault::{
    config::AppConfig,
    pkg::realtime::{
        bridge::{LocalBridge, RealtimeBridge},
        create_realtime_bridge, RealtimeHub,
    },
};

#[tokio::test]
async fn test_local_bridge_direct() {
    let bridge = LocalBridge::new();
    assert_eq!(bridge.name(), "local");

    let result = bridge.publish("chat", r#"{"type":"ping"}"#).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_realtime_hub_broadcast_and_subscribe() {
    let hub = RealtimeHub::new(16);
    let bridge = Arc::new(LocalBridge::new());
    hub.set_bridge(bridge).await;

    let mut rx1 = hub.subscribe();
    let mut rx2 = hub.subscribe();

    hub.broadcast("payload-alpha").await;

    let msg1 = rx1.recv().await.expect("rx1 should receive payload-alpha");
    let msg2 = rx2.recv().await.expect("rx2 should receive payload-alpha");

    assert_eq!(msg1, "payload-alpha");
    assert_eq!(msg2, "payload-alpha");
}

#[tokio::test]
async fn test_create_realtime_bridge_fallback_to_local() {
    let mut config = AppConfig::from_env();
    config.realtime_bridge = "local".to_string();

    let hub = RealtimeHub::new(16);
    let bridge = create_realtime_bridge(&config, hub.sender()).await;

    assert_eq!(bridge.name(), "local");
}
