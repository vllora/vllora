use opentelemetry::trace::{SpanId, TraceId, Tracer};
use opentelemetry_sdk::trace::{Config, IdGenerator, RandomIdGenerator};
use tracing::level_filters::LevelFilter;
use tracing::Subscriber;
use tracing_opentelemetry::PreSampledTracer;
use tracing_subscriber::filter;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;
use valuable::Valuable;

pub trait RecordResult: Sized {
    fn record(self) -> Self {
        self.record_with(&tracing::Span::current())
    }

    fn record_with(self, span: &tracing::Span) -> Self;
}

impl<T, E> RecordResult for Result<T, E>
where
    E: std::fmt::Display,
    T: Valuable,
{
    fn record_with(self, span: &tracing::Span) -> Self {
        match &self {
            Ok(result) => span.record("output", result.as_value()),
            Err(error) => span.record("error", error.to_string()),
        };
        self
    }
}

#[derive(Default, Debug)]
pub struct UuidIdGenerator(RandomIdGenerator);

impl IdGenerator for UuidIdGenerator {
    fn new_trace_id(&self) -> TraceId {
        TraceId::from_bytes(Uuid::new_v4().into_bytes())
    }

    fn new_span_id(&self) -> SpanId {
        self.0.new_span_id()
    }
}

pub fn config() -> Config {
    let mut config = Config::default();
    config.id_generator = Box::new(UuidIdGenerator::default());
    config
}

pub fn layer<S, T>(target: impl Into<String>, level: LevelFilter, tracer: T) -> impl Layer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    T: Tracer + PreSampledTracer + 'static,
{
    let target = target.into();
    tracing_opentelemetry::layer()
        .with_location(false)
        .with_tracked_inactivity(false)
        .with_threads(false)
        .with_tracer(tracer)
        .with_filter(filter::Targets::new().with_target(target, level))
}
