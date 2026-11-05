pub mod middleware;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::TracerProvider;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::AppConfig;

/// Initializes structured JSON logging and optional OpenTelemetry layer.
pub fn init_logger(config: &AppConfig, otel_provider: Option<&TracerProvider>) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "exavault={},tower_http=info",
            config.log_level
        ))
    });

    let otel_layer = otel_provider.map(|p| {
        let tracer = p.tracer(config.otel_service_name.clone());
        tracing_opentelemetry::layer().with_tracer(tracer)
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .with(otel_layer)
        .init();
}
