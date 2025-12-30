use opentelemetry::global;
use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter, Gauge};
use opentelemetry::KeyValue;
use opentelemetry::Context;
use opentelemetry::baggage::BaggageExt;
use std::sync::OnceLock;

/// Global meter instance for built-in metrics
static METER: OnceLock<Meter> = OnceLock::new();

/// Initialize the global meter (called during tracing initialization)
pub fn init_meter() {
    let meter = global::meter("vllora");
    METER.set(meter).expect("Meter should only be initialized once");
}

/// Get the global meter instance
fn get_meter() -> &'static Meter {
    METER.get().expect("Meter must be initialized before use")
}

/// Built-in metric instruments
pub struct BuiltInMetrics {
    /// Total number of LLM requests
    pub request_count: Counter<u64>,
    /// Request latency in milliseconds
    pub request_latency: Histogram<f64>,
    /// Time to first token in milliseconds
    pub request_ttft: Histogram<f64>,
    /// Tokens per second
    pub request_tps: Histogram<f64>,
    /// Input tokens counter
    pub input_tokens: Counter<u64>,
    /// Output tokens counter
    pub output_tokens: Counter<u64>,
    /// Total tokens counter
    pub total_tokens: Counter<u64>,
    /// Cost per request
    pub request_cost: Counter<f64>,
    /// Error count
    pub request_errors: Counter<u64>,
}

impl BuiltInMetrics {
    /// Create all built-in metric instruments
    pub fn new() -> Self {
        let meter = get_meter();
        
        Self {
            request_count: meter
                .u64_counter("llm.request.count")
                .with_description("Total number of LLM requests")
                .with_unit("requests")
                .build(),
            request_latency: meter
                .f64_histogram("llm.request.latency")
                .with_description("Request duration in milliseconds")
                .with_unit("ms")
                .build(),
            request_ttft: meter
                .f64_histogram("llm.request.ttft")
                .with_description("Time to first token in milliseconds")
                .with_unit("ms")
                .build(),
            request_tps: meter
                .f64_histogram("llm.request.tps")
                .with_description("Tokens per second")
                .with_unit("tokens/s")
                .build(),
            input_tokens: meter
                .u64_counter("llm.request.tokens.input")
                .with_description("Total input tokens")
                .with_unit("tokens")
                .build(),
            output_tokens: meter
                .u64_counter("llm.request.tokens.output")
                .with_description("Total output tokens")
                .with_unit("tokens")
                .build(),
            total_tokens: meter
                .u64_counter("llm.request.tokens.total")
                .with_description("Total tokens (input + output)")
                .with_unit("tokens")
                .build(),
            request_cost: meter
                .f64_counter("llm.request.cost")
                .with_description("Total cost per request")
                .with_unit("USD")
                .build(),
            request_errors: meter
                .u64_counter("llm.request.errors")
                .with_description("Total number of errors")
                .with_unit("errors")
                .build(),
        }
    }
}

/// Global instance of built-in metrics
static BUILT_IN_METRICS: OnceLock<BuiltInMetrics> = OnceLock::new();

/// Get the global built-in metrics instance
pub fn get_built_in_metrics() -> &'static BuiltInMetrics {
    BUILT_IN_METRICS.get_or_init(|| BuiltInMetrics::new())
}

/// Initialize built-in metrics (called after meter initialization)
pub fn init_built_in_metrics() {
    BUILT_IN_METRICS.get_or_init(|| BuiltInMetrics::new());
}

/// Extract attributes from the current OpenTelemetry context (baggage)
pub fn extract_span_attributes() -> Vec<KeyValue> {
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
    
    // Note: model_name and provider_name should be passed as additional_attrs
    // when recording metrics, as they're not in baggage
    
    attrs
}

/// Helper function to record metrics with span context attributes
pub fn record_with_context<F>(f: F)
where
    F: FnOnce(&[KeyValue]),
{
    let attrs = extract_span_attributes();
    f(&attrs);
}

/// Record latency metric
pub fn record_latency(latency_ms: f64, additional_attrs: &[KeyValue]) {
    let mut attrs = extract_span_attributes();
    attrs.extend_from_slice(additional_attrs);
    get_built_in_metrics().request_latency.record(latency_ms, &attrs);
}

/// Record TTFT metric
pub fn record_ttft(ttft_ms: f64, additional_attrs: &[KeyValue]) {
    let mut attrs = extract_span_attributes();
    attrs.extend_from_slice(additional_attrs);
    get_built_in_metrics().request_ttft.record(ttft_ms, &attrs);
}

/// Record TPS metric
pub fn record_tps(tps: f64, additional_attrs: &[KeyValue]) {
    let mut attrs = extract_span_attributes();
    attrs.extend_from_slice(additional_attrs);
    get_built_in_metrics().request_tps.record(tps, &attrs);
}

/// Record token metrics
pub fn record_tokens(input: u64, output: u64, additional_attrs: &[KeyValue]) {
    let mut attrs = extract_span_attributes();
    attrs.extend_from_slice(additional_attrs);
    get_built_in_metrics().input_tokens.add(input, &attrs);
    get_built_in_metrics().output_tokens.add(output, &attrs);
    get_built_in_metrics().total_tokens.add(input + output, &attrs);
}

/// Record cost metric
pub fn record_cost(cost: f64, additional_attrs: &[KeyValue]) {
    let mut attrs = extract_span_attributes();
    attrs.extend_from_slice(additional_attrs);
    get_built_in_metrics().request_cost.add(cost, &attrs);
}

/// Record error metric
pub fn record_error(additional_attrs: &[KeyValue]) {
    let mut attrs = extract_span_attributes();
    attrs.extend_from_slice(additional_attrs);
    get_built_in_metrics().request_errors.add(1, &attrs);
}

/// Record request count
pub fn record_request(additional_attrs: &[KeyValue]) {
    let mut attrs = extract_span_attributes();
    attrs.extend_from_slice(additional_attrs);
    get_built_in_metrics().request_count.add(1, &attrs);
}

/// Custom metrics API for user-defined metrics
pub struct CustomMetrics;

impl CustomMetrics {
    /// Create a custom counter metric
    pub fn counter(name: &str) -> CustomCounterBuilder {
        CustomCounterBuilder::new(name)
    }
    
    /// Create a custom histogram metric
    pub fn histogram(name: &str) -> CustomHistogramBuilder {
        CustomHistogramBuilder::new(name)
    }
    
    /// Create a custom gauge metric
    pub fn gauge(name: &str) -> CustomGaugeBuilder {
        CustomGaugeBuilder::new(name)
    }
    
    /// Create a custom up-down counter metric
    pub fn updown_counter(name: &str) -> CustomUpDownCounterBuilder {
        CustomUpDownCounterBuilder::new(name)
    }
}

/// Builder for custom counter metrics
pub struct CustomCounterBuilder {
    name: String,
    description: Option<String>,
    unit: Option<String>,
}

impl CustomCounterBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            unit: None,
        }
    }
    
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }
    
    pub fn with_unit(mut self, unit: &str) -> Self {
        self.unit = Some(unit.to_string());
        self
    }
    
    pub fn build_u64(self) -> Counter<u64> {
        let meter = get_meter();
        let name = self.name.clone();
        let mut builder = meter.u64_counter(name);
        if let Some(desc) = self.description {
            builder = builder.with_description(desc);
        }
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }
    
    pub fn build_f64(self) -> Counter<f64> {
        let meter = get_meter();
        let name = self.name.clone();
        let mut builder = meter.f64_counter(name);
        if let Some(desc) = self.description {
            builder = builder.with_description(desc);
        }
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }
}

/// Builder for custom histogram metrics
pub struct CustomHistogramBuilder {
    name: String,
    description: Option<String>,
    unit: Option<String>,
}

impl CustomHistogramBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            unit: None,
        }
    }
    
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }
    
    pub fn with_unit(mut self, unit: &str) -> Self {
        self.unit = Some(unit.to_string());
        self
    }
    
    pub fn build_u64(self) -> Histogram<u64> {
        let meter = get_meter();
        let name = self.name.clone();
        let mut builder = meter.u64_histogram(name);
        if let Some(desc) = self.description {
            builder = builder.with_description(desc);
        }
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }
    
    pub fn build_f64(self) -> Histogram<f64> {
        let meter = get_meter();
        let name = self.name.clone();
        let mut builder = meter.f64_histogram(name);
        if let Some(desc) = self.description {
            builder = builder.with_description(desc);
        }
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }
}

/// Builder for custom gauge metrics
pub struct CustomGaugeBuilder {
    name: String,
    description: Option<String>,
    unit: Option<String>,
}

impl CustomGaugeBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            unit: None,
        }
    }
    
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }
    
    pub fn with_unit(mut self, unit: &str) -> Self {
        self.unit = Some(unit.to_string());
        self
    }
    
    pub fn build_u64(self) -> Gauge<u64> {
        let meter = get_meter();
        let name = self.name.clone();
        let mut builder = meter.u64_gauge(name);
        if let Some(desc) = self.description {
            builder = builder.with_description(desc);
        }
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }
    
    pub fn build_f64(self) -> Gauge<f64> {
        let meter = get_meter();
        let name = self.name.clone();
        let mut builder = meter.f64_gauge(name);
        if let Some(desc) = self.description {
            builder = builder.with_description(desc);
        }
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }
}

/// Builder for custom up-down counter metrics
pub struct CustomUpDownCounterBuilder {
    name: String,
    description: Option<String>,
    unit: Option<String>,
}

impl CustomUpDownCounterBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            unit: None,
        }
    }
    
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }
    
    pub fn with_unit(mut self, unit: &str) -> Self {
        self.unit = Some(unit.to_string());
        self
    }
    
    pub fn build_i64(self) -> UpDownCounter<i64> {
        let meter = get_meter();
        let name = self.name.clone();
        let mut builder = meter.i64_up_down_counter(name);
        if let Some(desc) = self.description {
            builder = builder.with_description(desc);
        }
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }
    
    pub fn build_f64(self) -> UpDownCounter<f64> {
        let meter = get_meter();
        let name = self.name.clone();
        let mut builder = meter.f64_up_down_counter(name);
        if let Some(desc) = self.description {
            builder = builder.with_description(desc);
        }
        if let Some(unit) = self.unit {
            builder = builder.with_unit(unit);
        }
        builder.build()
    }
}
