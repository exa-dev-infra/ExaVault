use exavault::{
    config::AppConfig,
    pkg::telemetry::{init_telemetry, shutdown_telemetry},
};

#[test]
fn test_telemetry_disabled_returns_none() {
    let mut config = AppConfig::from_env();
    config.otel_enabled = false;

    let provider = init_telemetry(&config).expect("init_telemetry should not fail");
    assert!(provider.is_none());

    // Shutdown should not panic
    shutdown_telemetry();
}
