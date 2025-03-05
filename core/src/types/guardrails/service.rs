use crate::executor::context::ExecutorContext;
use crate::types::gateway::ChatCompletionRequest;
use crate::types::guardrails::Guard;
use crate::types::guardrails::GuardResult;

/// Trait for evaluating text against a guard
#[async_trait::async_trait]
pub trait GuardrailsEvaluator: Send + Sync {
    async fn evaluate(
        &self,
        request: &ChatCompletionRequest,
        guard: &Guard,
        executor_context: &ExecutorContext,
    ) -> Result<GuardResult, String>;
}
