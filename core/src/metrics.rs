use opentelemetry::global;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry::KeyValue;
use opentelemetry::Context;
use opentelemetry::baggage::BaggageExt;
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

/// Extract attributes from the current OpenTelemetry context (baggage)
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
    
    attrs
}

/// Record latency metric
pub fn record_latency(latency_ms: f64, model_name: &str, provider_name: &str) {
    let meter = get_meter();
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    
    let histogram: Histogram<f64> = meter
        .f64_histogram("llm.request.latency")
        .with_description("Request duration in milliseconds")
        .with_unit("ms")
        .build();
    
    histogram.record(latency_ms, &attrs);
}

/// Record TTFT metric
pub fn record_ttft(ttft_us: u64, model_name: &str, provider_name: &str) {
    let meter = get_meter();
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    
    // Convert microseconds to milliseconds
    let ttft_ms = ttft_us as f64 / 1000.0;
    
    let histogram: Histogram<f64> = meter
        .f64_histogram("llm.request.ttft")
        .with_description("Time to first token in milliseconds")
        .with_unit("ms")
        .build();
    
    histogram.record(ttft_ms, &attrs);
}

/// Record TPS metric
pub fn record_tps(tps: f64, model_name: &str, provider_name: &str) {
    let meter = get_meter();
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    
    let histogram: Histogram<f64> = meter
        .f64_histogram("llm.request.tps")
        .with_description("Tokens per second")
        .with_unit("tokens/s")
        .build();
    
    histogram.record(tps, &attrs);
}

/// Record token metrics
pub fn record_tokens(input: u64, output: u64, model_name: &str, provider_name: &str) {
    let meter = get_meter();
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    
    let input_counter: Counter<u64> = meter
        .u64_counter("llm.request.tokens.input")
        .with_description("Total input tokens")
        .with_unit("tokens")
        .build();
    
    let output_counter: Counter<u64> = meter
        .u64_counter("llm.request.tokens.output")
        .with_description("Total output tokens")
        .with_unit("tokens")
        .build();
    
    let total_counter: Counter<u64> = meter
        .u64_counter("llm.request.tokens.total")
        .with_description("Total tokens (input + output)")
        .with_unit("tokens")
        .build();
    
    input_counter.add(input, &attrs);
    output_counter.add(output, &attrs);
    total_counter.add(input + output, &attrs);
}

/// Record cost metric
pub fn record_cost(cost: f64, model_name: &str, provider_name: &str) {
    let meter = get_meter();
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    
    let counter: Counter<f64> = meter
        .f64_counter("llm.request.cost")
        .with_description("Total cost per request")
        .with_unit("USD")
        .build();
    
    counter.add(cost, &attrs);
}

/// Record error metric
pub fn record_error(model_name: &str, provider_name: &str) {
    let meter = get_meter();
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    
    let counter: Counter<u64> = meter
        .u64_counter("llm.request.errors")
        .with_description("Total number of failed LLM requests")
        .with_unit("errors")
        .build();
    
    counter.add(1, &attrs);
}

/// Record request count
pub fn record_request(model_name: &str, provider_name: &str) {
    let meter = get_meter();
    let mut attrs = extract_context_attributes();
    attrs.push(KeyValue::new("model_name", model_name.to_string()));
    attrs.push(KeyValue::new("provider_name", provider_name.to_string()));
    
    let counter: Counter<u64> = meter
        .u64_counter("llm.request.count")
        .with_description("Total number of LLM requests")
        .with_unit("requests")
        .build();
    
    counter.add(1, &attrs);
}
