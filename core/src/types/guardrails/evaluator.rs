use crate::types::gateway::ChatCompletionRequest;
use crate::types::guardrails::Guard;
use crate::types::guardrails::GuardResult;

/// Trait for evaluating text against a guard
#[async_trait::async_trait]
pub trait Evaluator: Send + Sync {
    async fn evaluate(
        &self,
        request: &ChatCompletionRequest,
        guard: &Guard,
    ) -> Result<GuardResult, String>;

    fn request_to_text(&self, request: &ChatCompletionRequest) -> Result<String, String> {
        let text = request
            .messages
            .last()
            .ok_or("No message in request")?
            .content
            .as_ref()
            .ok_or("No content in message")?
            .as_string()
            .ok_or("No text in content")?;

        Ok(text)
    }
}
