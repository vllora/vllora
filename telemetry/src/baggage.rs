use opentelemetry::{baggage::BaggageExt, global::ObjectSafeSpan};
use opentelemetry_sdk::{error::OTelSdkError, trace::SpanProcessor};

#[derive(Debug)]
pub struct BaggageSpanProcessor<const N: usize> {
    keys: [&'static str; N],
}

impl<const N: usize> BaggageSpanProcessor<N> {
    pub const fn new(keys: [&'static str; N]) -> Self {
        Self { keys }
    }
}

impl<const N: usize> SpanProcessor for BaggageSpanProcessor<N> {
    fn on_start(&self, span: &mut opentelemetry_sdk::trace::Span, cx: &opentelemetry::Context) {
        for key in self.keys {
            let value = cx.baggage().get(key);
            if let Some(value) = value {
                span.set_attribute(opentelemetry::KeyValue::new(key, value.clone()));
            }
        }
    }

    fn on_end(&self, _span: opentelemetry_sdk::trace::SpanData) {}

    fn force_flush(&self) -> std::result::Result<(), OTelSdkError> {
        Ok(())
    }

    fn shutdown(&self) -> std::result::Result<(), OTelSdkError> {
        Ok(())
    }

    fn shutdown_with_timeout(
        &self,
        _timeout: std::time::Duration,
    ) -> std::result::Result<(), OTelSdkError> {
        Ok(())
    }
}
