use crate::executor::context::ExecutorContext;
use crate::types::gateway::ChatCompletionRequest;
use crate::types::guardrails::GuardResult;

use super::GuardStage;

/// Trait for evaluating text against a guard
#[async_trait::async_trait]
pub trait GuardrailsEvaluator: Send + Sync {
    async fn evaluate(
        &self,
        request: &ChatCompletionRequest,
        guard_id: &str,
        executor_context: &ExecutorContext,
        parameters: Option<&serde_json::Value>,
        evaluation_stage: &GuardStage,
    ) -> Result<GuardResult, String>;
}
