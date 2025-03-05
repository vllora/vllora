use langdb_core::events::JsonValue;
use langdb_core::events::SPAN_GUARD_EVAULATION;
use langdb_core::types::gateway::ChatCompletionRequest;
use langdb_core::types::guardrails::evaluator::Evaluator;
use langdb_core::types::guardrails::Guard;
use langdb_core::types::guardrails::GuardResult;
use tracing::field;
use tracing::info_span;
use tracing_futures::Instrument;
use valuable::Valuable;

pub struct TracedGuard {
    inner: Box<dyn Evaluator>,
}

// Implement Send + Sync since inner is already Send + Sync
unsafe impl Send for TracedGuard {}
unsafe impl Sync for TracedGuard {}

impl TracedGuard {
    pub fn new(inner: Box<dyn Evaluator>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl Evaluator for TracedGuard {
    async fn evaluate(
        &self,
        request: &ChatCompletionRequest,
        guard: &Guard,
    ) -> Result<GuardResult, String> {
        let guard_value = serde_json::to_value(guard.clone()).map_err(|e| e.to_string())?;

        let span = info_span!(
            target: "langdb::user_tracing::guard",
            SPAN_GUARD_EVAULATION,
            guard = JsonValue(&guard_value).as_value(),
            id = guard.id,
            label = guard.name,
            result = field::Empty
        );

        let result = self
            .inner
            .evaluate(request, guard)
            .instrument(span.clone())
            .await;
        let result_value = serde_json::to_value(result.clone()).map_err(|e| e.to_string())?;
        span.record("result", JsonValue(&result_value).as_value());

        result
    }
}
