use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::Resource;
use std::sync::Arc;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Layer, Registry};
use vllora_core::telemetry::ProjectTraceSpanExporter;
use vllora_core::telemetry::RunSpanBuffer;
use vllora_core::telemetry::RunSpanBufferExporter;
use vllora_telemetry::events::{self, BaggageSpanProcessor};
use vllora_telemetry::ProjectTraceMap;

use crate::metrics;

pub fn init_tracing(
    project_trace_senders: Arc<ProjectTraceMap>,
    run_span_buffer: Arc<RunSpanBuffer>,
) {
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

    // Initialize tracing (spans)
    let otlp_span_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()
        .unwrap();
    let project_trace_span_exporter = ProjectTraceSpanExporter::new(project_trace_senders);
    let run_span_buffer_exporter = RunSpanBufferExporter::new(run_span_buffer);

    let trace_provider = SdkTracerProvider::builder()
        .with_span_processor(BaggageSpanProcessor::new([
            "vllora.run_id",
            "vllora.thread_id",
            "vllora.label",
            "vllora.tenant",
            "vllora.project_id",
        ]))
        .with_simple_exporter(run_span_buffer_exporter)
        .with_simple_exporter(project_trace_span_exporter)
        .with_batch_exporter(otlp_span_exporter)
        .with_id_generator(events::UuidIdGenerator::default())
        .build();
    let tracer = trace_provider.tracer("vllora");
    opentelemetry::global::set_tracer_provider(trace_provider);

    // Initialize metrics
    let resource = Resource::builder()
        .with_attributes(vec![
            opentelemetry::KeyValue::new("service.name", "vllora"),
        ])
        .build();
    
    let otlp_metrics_exporter = opentelemetry_otlp::MetricExporterBuilder::new()
        .with_tonic()
        .build();
    
    if let Ok(exporter) = otlp_metrics_exporter {
        let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(exporter)
            .with_interval(std::time::Duration::from_secs(10))
            .build();
        
        let meter_provider = SdkMeterProvider::builder()
            .with_resource(resource)
            .with_reader(reader)
            .build();
        opentelemetry::global::set_meter_provider(meter_provider);
        
        // Initialize metrics module
        metrics::init_meter();
        metrics::init_built_in_metrics();
        
        tracing::info!("Metrics provider initialized successfully");
    } else {
        tracing::warn!("Failed to initialize metrics exporter. Metrics will not be exported via OTLP.");
    }

    let otel_layer = events::layer("vllora::user_tracing", LevelFilter::INFO, tracer);
    Registry::default()
        .with(builder)
        .with(otel_layer)
        .try_init()
        .expect("initialized subscriber successfully");
}
