//! Example demonstrating how to use vllora_telemetry span macros and tracing exporter
//!
//! This example shows:
//! 1. How to set up OpenTelemetry tracing with OTLP exporter
//! 2. How to use the span macros (create_model_span, create_api_invoke_span, etc.)
//! 3. How to create and instrument spans with attributes
//! 4. How to export traces to an OTLP endpoint
//!
//! Run with:
//! ```bash
//! cargo run --example tracing_example -- --otlp-endpoint http://localhost:4317
//! ```

use clap::Parser;
use opentelemetry::baggage::BaggageExt;
use opentelemetry::global;
use opentelemetry::trace::FutureExt;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{Context, KeyValue};
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::trace::SpanData;
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;
use tracing::field;
use tracing::level_filters::LevelFilter;
use tracing::Instrument;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry};
use valuable::Valuable;
use vllora_telemetry::baggage::BaggageSpanProcessor;
use vllora_telemetry::events::{self, JsonValue};
// Import span creation macros - these are exported via #[macro_export] in events/span.rs
use vllora_telemetry::{
    create_agent_span, create_api_invoke_span, create_model_invoke_span, create_model_span,
    create_run_span, create_task_span, create_tool_span,
};

/// A simple console exporter that prints spans using tracing::info!
#[derive(Clone)]
struct ConsoleSpanExporter;

impl fmt::Debug for ConsoleSpanExporter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsoleSpanExporter").finish()
    }
}

impl ConsoleSpanExporter {
    fn new() -> Self {
        Self
    }
}

impl opentelemetry_sdk::trace::SpanExporter for ConsoleSpanExporter {
    async fn export(&self, batch: Vec<SpanData>) -> opentelemetry_sdk::error::OTelSdkResult {
        for span in batch {
            let trace_id = span.span_context.trace_id();
            let span_id = span.span_context.span_id();
            let parent_span_id = span.parent_span_id;
            let duration = span
                .end_time
                .duration_since(span.start_time)
                .unwrap_or_default();

            let mut attrs = String::new();
            for attr in &span.attributes {
                attrs.push_str(&format!("{}={:?}, ", attr.key, attr.value));
            }
            if attrs.ends_with(", ") {
                attrs.truncate(attrs.len() - 2);
            }

            println!(
                // target: "console_exporter",
                "\n[Console Exporter] Span: {} (trace_id={}, span_id={}, parent_span_id={}, duration={:?})",
                span.name,
                trace_id,
                span_id,
                parent_span_id,
                duration
            );

            if !attrs.is_empty() {
                tracing::info!(target: "console_exporter", "  Attributes: {}", attrs);
            }

            tracing::info!(target: "console_exporter", "  Status: {:?}", span.status);
        }
        opentelemetry_sdk::error::OTelSdkResult::Ok(())
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// OTLP endpoint (e.g., http://localhost:4317)
    #[arg(long, default_value = "http://localhost:4317")]
    otlp_endpoint: String,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

fn init_tracing(otlp_endpoint: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Set OTLP endpoint via environment variable
    std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", otlp_endpoint);

    // Set up OTLP exporter
    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()?;

    // Set up console exporter
    let console_exporter = ConsoleSpanExporter::new();

    // Create tracer provider with both OTLP and console exporters
    let trace_provider = SdkTracerProvider::builder()
        .with_span_processor(BaggageSpanProcessor::new([
            "vllora.run_id",
            "vllora.thread_id",
            "vllora.label",
            "vllora.tenant",
            "vllora.project_id",
        ]))
        .with_simple_exporter(console_exporter)
        .with_batch_exporter(otlp_exporter)
        .with_id_generator(events::UuidIdGenerator::default())
        .build();

    let tracer = trace_provider.tracer("tracing_example");
    global::set_tracer_provider(trace_provider);

    // Set up tracing subscriber with OpenTelemetry layer
    // let otel_layer = events::layer("vllora::user_tracing,app::runtime", LevelFilter::INFO, tracer);
    let vllora_layer = events::layer("vllora::user_tracing", LevelFilter::INFO, tracer.clone());
    let custom_layer = events::layer("app::runtime", LevelFilter::INFO, tracer);

    // Set up fmt layer with env filter for RUST_LOG support
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter = EnvFilter::new(log_level);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    Registry::default()
        .with(env_filter)
        .with(fmt_layer)
        .with(vllora_layer)
        .with(custom_layer)
        .try_init()
        .expect("Failed to initialize tracing subscriber");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Set RUST_LOG if not already set (allows user to override with environment variable)
    if std::env::var("RUST_LOG").is_err() {
        if args.verbose {
            std::env::set_var("RUST_LOG", "debug");
        } else {
            std::env::set_var("RUST_LOG", "info");
        }
    }

    // Initialize tracing with OTLP and console exporters
    init_tracing(&args.otlp_endpoint)?;

    tracing::info!("Tracing initialized.");
    tracing::info!("  - Console exporter: enabled (spans will be printed to stdout)");
    tracing::info!(
        "  - OTLP exporter: enabled (sending to: {})",
        args.otlp_endpoint
    );
    tracing::info!("\nStarting example spans...\n");

    // Set up context with baggage values (run_id and thread_id)
    // These will be propagated to all spans by BaggageSpanProcessor
    let run_id = uuid::Uuid::new_v4().to_string();
    let thread_id = uuid::Uuid::new_v4().to_string();

    let context = Context::current().with_baggage(vec![
        KeyValue::new("vllora.run_id", run_id.clone()),
        KeyValue::new("vllora.thread_id", thread_id.clone()),
        KeyValue::new("vllora.tenant", "default".to_string()),
    ]);

    tracing::info!(
        "Set context baggage: run_id={}, thread_id={}",
        run_id,
        thread_id
    );

    // Attach context BEFORE executing flow so BaggageSpanProcessor can read it
    execute_flow().with_context(context).await;

    // Give time for traces to be exported
    tracing::info!("\nWaiting for traces to be exported...");
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Drop the tracer provider to flush remaining spans
    drop(global::tracer_provider());

    tracing::info!("Example completed! Check your OTLP endpoint for traces.");

    Ok(())
}

async fn execute_flow() {
    let span = tracing::info_span!(
        target: "app::runtime::execution",
        "execution",
        execution_id = 123,
    );

    // Hierarchy: run -> agent -> task -> thread -> model_invoke -> openai -> tool
    run_operation().instrument(span).await;
}

// Helper functions for span hierarchy
// Hierarchy: run -> agent -> task -> thread -> model_invoke -> openai -> tool

async fn run_operation() {
    let run_span = create_run_span!({
        let mut tags = HashMap::new();
        tags.insert("environment".to_string(), "example".to_string());
        tags
    });

    tracing::info!("1. Created run span");
    run_operation_inner().instrument(run_span).await;
}

async fn run_operation_inner() {
    agent_operation().await;
}

async fn agent_operation() {
    let agent_span = create_agent_span!("research_assistant");

    tracing::info!("2. Created agent span");
    agent_operation_inner().instrument(agent_span).await;
}

async fn agent_operation_inner() {
    task_operation().await;
}

async fn task_operation() {
    let task_span = create_task_span!("gather_information");

    tracing::info!("3. Created task span");
    task_operation_inner().instrument(task_span).await;
}

async fn task_operation_inner() {
    thread_operation().await;
}

async fn thread_operation() {
    let thread_tags = {
        let mut tags = HashMap::new();
        tags.insert("user_id".to_string(), "user123".to_string());
        tags.insert("session_id".to_string(), "session456".to_string());
        tags
    };

    let thread_span = create_api_invoke_span!(thread_tags);

    tracing::info!("4. Created thread span");
    thread_operation_inner().instrument(thread_span).await;
}

async fn thread_operation_inner() {
    model_invoke_operation().await;
}

async fn model_invoke_operation() {
    let model_tags = {
        let mut tags = HashMap::new();
        tags.insert("model_type".to_string(), "completion".to_string());
        tags
    };

    let model_json = serde_json::json!({
        "name": "gpt-4",
        "provider_name": "openai",
        "engine_name": "openai",
        "model_params": {
            "engine": {
                "OpenAi": {
                    "params": {
                        "temperature": 0.7
                    }
                }
            },
            "provider_name": "openai"
        },
        "model_name": "gpt-4",
        "tools": [],
        "model_type": "completions"
    });

    let model_invoke_span = create_model_invoke_span!(
        "{}",                                        // input
        serde_json::to_string(&model_json).unwrap(), // model JSON
        "openai",                                    // provider_name
        "gpt-4",                                     // model_name
        "gpt-4",                                     // inference_model_name
        "own",                                       // credentials_identifier
        model_tags.clone()
    );

    tracing::info!("5. Created model invoke span");

    // Record some attributes on the model invoke span
    model_invoke_span.record("input", "What is the capital of France?");
    model_invoke_span.record("output", "The capital of France is Paris.");
    model_invoke_span.record(
        "usage",
        JsonValue(&serde_json::json!({
            "input_tokens": 10,
            "output_tokens": 8,
            "total_tokens": 18
        }))
        .as_value(),
    );

    // Record cost as JSON string
    let cost_json = serde_json::json!({
        "cost": 0.000301,
        "per_input_token": 1.0,
        "per_cached_input_token": 0.5,
        "per_cached_input_write_token": 0.0,
        "per_output_token": 3.0,
        "is_cache_used": false
    });
    model_invoke_span.record("cost", serde_json::to_string(&cost_json).unwrap());

    model_invoke_span.record("ttft", 150u64); // Time to first token in microseconds

    model_invoke_operation_inner()
        .instrument(model_invoke_span)
        .await;
}

async fn model_invoke_operation_inner() {
    openai_operation().await;
}

async fn openai_operation() {
    let openai_tags = {
        let mut tags = HashMap::new();
        tags.insert("provider".to_string(), "openai".to_string());
        tags
    };

    let openai_span = create_model_span!(
        vllora_telemetry::events::SPAN_OPENAI,
        "vllora::user_tracing::openai",
        openai_tags,
        0
    );

    tracing::info!("6. Created openai span");

    // Record fields on the openai span
    // Record request
    let request_json = serde_json::json!({
        "messages": [{"role": "user", "content": "Hello"}],
        "model": "gpt-4o-mini",
        "stream": true,
        "stream_options": {"include_usage": true}
    });
    openai_span.record("request", serde_json::to_string(&request_json).unwrap());

    // Record input
    let input_json = serde_json::json!([{"role": "user", "content": "Hello"}]);
    openai_span.record("input", serde_json::to_string(&input_json).unwrap());

    // Record retries_left
    openai_span.record("retries_left", 0u64);

    // Record usage
    let usage_json = serde_json::json!({
        "input_tokens": 8,
        "output_tokens": 9,
        "total_tokens": 17,
        "prompt_tokens_details": {
            "cached_tokens": 0,
            "cache_creation_tokens": 0,
            "audio_tokens": 0
        },
        "completion_tokens_details": {
            "accepted_prediction_tokens": 0,
            "audio_tokens": 0,
            "reasoning_tokens": 0,
            "rejected_prediction_tokens": 0
        },
        "is_cache_used": false
    });
    openai_span.record("usage", serde_json::to_string(&usage_json).unwrap());

    // Record raw_usage
    let raw_usage_json = serde_json::json!({
        "prompt_tokens": 8,
        "completion_tokens": 9,
        "total_tokens": 17,
        "prompt_tokens_details": {
            "audio_tokens": 0,
            "cached_tokens": 0
        },
        "completion_tokens_details": {
            "accepted_prediction_tokens": 0,
            "audio_tokens": 0,
            "reasoning_tokens": 0,
            "rejected_prediction_tokens": 0
        }
    });
    openai_span.record("raw_usage", serde_json::to_string(&raw_usage_json).unwrap());

    // Record cost
    openai_span.record("cost", 6.600000262260437e-6f64);

    // Record output
    let output_json = serde_json::json!({
        "id": "chatcmpl-Cs1HXW4hkAGDQjlv5F8REozH9V0ZL",
        "choices": [{
            "index": 0,
            "message": {
                "content": "Hello! How can I assist you today?",
                "tool_calls": [],
                "role": "assistant"
            },
            "finish_reason": "stop"
        }],
        "created": 1766990675,
        "model": "gpt-4o-mini-2024-07-18",
        "service_tier": "default",
        "system_fingerprint": "fp_644f11dd4d",
        "object": "chat.completion",
        "usage": {
            "prompt_tokens": 8,
            "completion_tokens": 9,
            "total_tokens": 17,
            "prompt_tokens_details": {
                "audio_tokens": 0,
                "cached_tokens": 0
            },
            "completion_tokens_details": {
                "accepted_prediction_tokens": 0,
                "audio_tokens": 0,
                "reasoning_tokens": 0,
                "rejected_prediction_tokens": 0
            }
        }
    });
    openai_span.record("output", serde_json::to_string(&output_json).unwrap());

    openai_operation_inner().instrument(openai_span).await;
}

async fn openai_operation_inner() {
    // Simulate some work (simulating API call)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Record TTFT (Time to First Token) - this needs to be recorded after the async work
    let current_span = tracing::Span::current();
    current_span.record("ttft", 843129u64);

    tool_operation().await;
}

async fn tool_operation() {
    let tool_tags = {
        let mut tags = HashMap::new();
        tags.insert("tool_type".to_string(), "function".to_string());
        tags
    };

    let tool_span = create_tool_span!("search_web", tool_tags);

    tracing::info!("7. Created tool span");
    tool_span.record("input", "search query: Paris");
    tool_span.record("output", "Search results...");
    tool_operation_inner().instrument(tool_span).await;
}

async fn tool_operation_inner() {
    tokio::time::sleep(Duration::from_millis(50)).await;
}
