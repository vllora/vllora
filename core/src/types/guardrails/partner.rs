use crate::types::guardrails::GuardResult;
use vllora_llm::client::error::AuthorizationError;
use vllora_llm::types::gateway::ChatCompletionMessage;

#[derive(Debug, thiserror::Error)]
pub enum GuardPartnerError {
    #[error("Invalid API key")]
    InvalidApiKey(#[from] AuthorizationError),

    #[error("Failed to evaluate guard")]
    EvaluationFailed(String),

    #[error("Input type {0}not supported")]
    InputTypeNotSupported(String),

    #[error("Input image is missing")]
    InputImageIsMissing,

    #[error(transparent)]
    BoxedError(#[from] Box<dyn std::error::Error>),
}
#[async_trait::async_trait]
pub trait GuardPartner {
    async fn evaluate(
        &self,
        messages: &[ChatCompletionMessage],
    ) -> Result<GuardResult, GuardPartnerError>;
}
