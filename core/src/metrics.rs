use opentelemetry::global;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry::KeyValue;
use opentelemetry::Context;
use opentelemetry::baggage::BaggageExt;
use opentelemetry::trace::TraceContextExt;
use std::sync::OnceLock;

/// Global meter instance for metrics
static METER: OnceLock<Meter> = OnceLock::new();

/// Initialize the global meter (called during tracing initialization)
pub fn init_meter() {
    let meter = global::meter("vllora");
    METER.set(meter).expect("Meter should only be initialized once");
}

/// Get the global meter instance
fn get_meter() -> &'static Meter {
    METER.get().unwrap_or_else(|| {
        // Fallback: initialize meter if not already initialized
        let meter = global::meter("vllora");
        METER.set(meter).ok();
        METER.get().expect("Meter should be initialized")
    })
}

/// Extract attributes from the current OpenTelemetry context (baggage and span)
fn extract_context_attributes() -> Vec<KeyValue> {
    let mut attrs = Vec::new();
    let ctx = Context::current();
    
    // Extract from baggage (set by BaggageSpanProcessor)
    let baggage = ctx.baggage();
    if let Some(project_id) = baggage.get("vllora.project_id") {
        attrs.push(KeyValue::new("project_id", project_id.to_string()));
    }
    
    if let Some(thread_id) = baggage.get("vllora.thread_id") {
        attrs.push(KeyValue::new("thread_id", thread_id.to_string()));
    }
    
    if let Some(run_id) = baggage.get("vllora.run_id") {
        attrs.push(KeyValue::new("run_id", run_id.to_string()));
    }
    
    // Extract trace_id and span_id from current span context
    let span = ctx.span();
    let span_context = span.span_context();
    if span_context.is_valid() {
        attrs.push(KeyValue::new("trace_id", span_context.trace_id().to_string()));
        attrs.push(KeyValue::new("span_id", span_context.span_id().to_string()));
    }
    
    attrs
}

// Reusable metric instruments (created once, reused many times)
// Using OnceLock to ensure they're only created once
static LATENCY_HISTOGRAM: OnceLock<Histogram<f64>> = OnceLock::new();
static TTFT_HISTOGRAM: OnceLock<Histogram<f64>> = OnceLock::new();
static TPS_HISTOGRAM: OnceLock<Histogram<f64>> = OnceLock::new();
static INPUT_TOKENS_COUNTER: OnceLock<Counter<u64>> = OnceLock::new();
static OUTPUT_TOKENS_COUNTER: OnceLock<Counter<u64>> = OnceLock::new();
static TOTAL_TOKENS_COUNTER: OnceLock<Counter<u64>> = OnceLock::new();
static COST_COUNTER: OnceLock<Counter<f64>> = OnceLock::new();
static ERROR_COUNTER: OnceLock<Counter<u64>> = OnceLock::new();
static REQUEST_COUNTER: OnceLock<Counter<u64>> = OnceLock::new();

fn get_latency_histogram() -> &'static Histogram<f64> {
    LATENCY_HISTOGRAM.get_or_init(|| {
        global::meter("vllora")
            .f64_histogram("llm.request.latency")
            .with_description("Request duration in milliseconds")
            .with_unit("ms")
            .build()
    })
}

fn get_ttft_histogram() -> &'static Histogram<f64> {
    TTFT_HISTOGRAM.get_or_init(|| {
        global::meter("vllora")
            .f64_histogram("llm.request.ttft")
            .with_description("Time to first token in milliseconds")
            .with_unit("ms")
            .build()
    })
}

fn get_tps_histogram() -> &'static Histogram<f64> {
    TPS_HISTOGRAM.get_or_init(|| {
        global::meter("vllora")
            .f64_histogram("llm.request.tps")
            .with_description("Tokens per second")
            .with_unit("tokens/s")
            .build()
    })
}

fn get_input_tokens_counter() -> &'static Counter<u64> {
    INPUT_TOKENS_COUNTER.get_or_init(|| {
        global::meter("vllora")
            .u64_counter("llm.request.tokens.input")
            .with_description("Total input tokens")
            .with_unit("tokens")
            .build()
    })
}

fn get_output_tokens_counter() -> &'static Counter<u64> {
    OUTPUT_TOKENS_COUNTER.get_or_init(|| {
        global::meter("vllora")
            .u64_counter("llm.request.tokens.output")
            .with_description("Total output tokens")
            .with_unit("tokens")
            .build()
    })
}

fn get_total_tokens_counter() -> &'static Counter<u64> {
    TOTAL_TOKENS_COUNTER.get_or_init(|| {
        global::meter("vllora")
            .u64_counter("llm.request.tokens.total")
            .with_description("Total tokens (input + output)")
            .with_unit("tokens")
            .build()
    })
}

fn get_cost_counter() -> &'static Counter<f64> {
    COST_COUNTER.get_or_init(|| {
        global::meter("vllora")
            .f64_counter("llm.request.cost")
            .with_description("Total cost per request")
            .with_unit("USD")
            .build()
    })
}

fn get_error_counter() -> &'static Counter<u64> {
    ERROR_COUNTER.get_or_init(|| {
        global::meter("vllora")
            .u64_counter("llm.request.errors")
            .with_description("Total number of failed LLM requests")
            .with_unit("errors")
            .build()
    })
}

fn get_request_counter() -> &'static Counter<u64> {
    REQUEST_COUNTER.get_or_init(|| {
        global::meter("vllora")
            .u64_counter("llm.request.count")
            .with_description("Total number of LLM requests")
            .with_unit("requests")
            .build()
    })
}

/// Record latency metric
pub fn record_latency(latency_ms: f64, model_name: &str, provider_name: &str) {
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    get_latency_histogram().record(latency_ms, &attrs);
}

/// Record TTFT metric
pub fn record_ttft(ttft_us: u64, model_name: &str, provider_name: &str) {
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    
    // Convert microseconds to milliseconds
    let ttft_ms = ttft_us as f64 / 1000.0;
    get_ttft_histogram().record(ttft_ms, &attrs);
}

/// Record TPS metric
pub fn record_tps(tps: f64, model_name: &str, provider_name: &str) {
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    get_tps_histogram().record(tps, &attrs);
}

/// Record token metrics
pub fn record_tokens(input: u64, output: u64, model_name: &str, provider_name: &str) {
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    
    get_input_tokens_counter().add(input, &attrs);
    get_output_tokens_counter().add(output, &attrs);
    get_total_tokens_counter().add(input + output, &attrs);
}

/// Record cost metric
pub fn record_cost(cost: f64, model_name: &str, provider_name: &str) {
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    get_cost_counter().add(cost, &attrs);
}

/// Record error metric
pub fn record_error(model_name: &str, provider_name: &str) {
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    get_error_counter().add(1, &attrs);
}

/// Record request count
pub fn record_request(model_name: &str, provider_name: &str) {
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    get_request_counter().add(1, &attrs);
}
