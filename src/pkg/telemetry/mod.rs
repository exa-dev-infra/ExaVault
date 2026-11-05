use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    trace::{BatchSpanProcessor, TracerProvider},
    Resource,
};
use tracing::info;

use crate::config::AppConfig;

/// Initializes OpenTelemetry tracer provider if enabled in AppConfig.
pub fn init_telemetry(
    config: &AppConfig,
) -> Result<Option<TracerProvider>, Box<dyn std::error::Error>> {
    if !config.otel_enabled {
        return Ok(None);
    }

    info!(
        service = %config.otel_service_name,
        endpoint = %config.otel_exporter_endpoint,
        "Initializing OpenTelemetry Tracer with OTLP gRPC exporter..."
    );

    // 1. W3C Trace Context Propagator
    global::set_text_map_propagator(TraceContextPropagator::new());

    // 2. OTLP Span Exporter via Tonic gRPC
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.otel_exporter_endpoint)
        .build()?;

    // 3. Resource Attributes (service.name)
    let resource = Resource::new(vec![KeyValue::new(
        "service.name",
        config.otel_service_name.clone(),
    )]);

    // 4. Batch Span Processor & Tracer Provider
    let provider = TracerProvider::builder()
        .with_span_processor(
            BatchSpanProcessor::builder(exporter, opentelemetry_sdk::runtime::Tokio).build(),
        )
        .with_resource(resource)
        .build();

    global::set_tracer_provider(provider.clone());

    info!("OpenTelemetry tracing provider successfully initialized");
    Ok(Some(provider))
}

/// Flushes and cleanly shuts down the OpenTelemetry tracer provider.
pub fn shutdown_telemetry() {
    global::shutdown_tracer_provider();
}
