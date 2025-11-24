use vllora_llm::types::gateway::ChatCompletionMessage;

use crate::executor::context::ExecutorContext;
use crate::types::guardrails::GuardResult;

use super::GuardStage;

/// Trait for evaluating text against a guard
#[async_trait::async_trait]
pub trait GuardrailsEvaluator: Send + Sync {
    async fn evaluate(
        &self,
        messages: &[ChatCompletionMessage],
        guard_id: &str,
        executor_context: &ExecutorContext,
        parameters: Option<&serde_json::Value>,
        guard_stage: &GuardStage,
    ) -> Result<GuardResult, String>;
}
