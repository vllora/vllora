use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::sync::Arc;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Layer, Registry};
use vllora_core::telemetry::events::{self, BaggageSpanProcessor};
use vllora_core::telemetry::ProjectTraceMap;
use vllora_core::telemetry::ProjectTraceSpanExporter;

pub fn init_tracing(project_trace_senders: Arc<ProjectTraceMap>) {
    let log_level = std::env::var("RUST_LOG").unwrap_or("info".to_string());
    let env_filter = EnvFilter::new(log_level).add_directive("actix_server=off".parse().unwrap());
    let color = std::env::var("ANSI_OUTPUT").map_or(true, |v| v == "true");

    // tracing syntax ->
    let builder = tracing_subscriber::fmt::layer()
        .compact()
        .with_line_number(false)
        .with_file(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_target(false)
        .with_ansi(color)
        .with_filter(env_filter);

    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
        .unwrap();
    let project_trace_span_exporter = ProjectTraceSpanExporter::new(project_trace_senders);

    let provider = SdkTracerProvider::builder()
        .with_span_processor(BaggageSpanProcessor::new([
            "vllora.run_id",
            "vllora.thread_id",
            "vllora.label",
            "vllora.tenant",
            "vllora.project_id",
        ]))
        .with_simple_exporter(project_trace_span_exporter)
        .with_batch_exporter(otlp_exporter)
        .with_id_generator(events::UuidIdGenerator::default())
        .build();
    let tracer = provider.tracer("langdb-ai-gateway");
    opentelemetry::global::set_tracer_provider(provider);

    let otel_layer = events::layer("vllora::user_tracing", LevelFilter::INFO, tracer);
    Registry::default()
        .with(builder)
        .with(otel_layer)
        .try_init()
        .expect("initialized subscriber successfully");
}
