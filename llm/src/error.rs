use crate::client::error::ModelError;
use crate::client::message_mapper::MessageMapperError;
use crate::{mcp::McpServerError, types::ModelEvent};
use thiserror::Error;

pub type LLMResult<T> = Result<T, LLMError>;

#[derive(Error, Debug)]
pub enum LLMError {
    #[error("Missing variable {0}")]
    MissingVariable(String),
    #[error(transparent)]
    StdIOError(#[from] std::io::Error),
    #[error(transparent)]
    ParseError(#[from] serde_json::Error),
    #[error("Error decoding argument: {0}")]
    DecodeError(#[from] base64::DecodeError),
    #[error("Custom Error: {0}")]
    CustomError(String),
    #[error("Api key was ")]
    InvalidCredentials,
    #[error("Function get is not implemented")]
    FunctionGetNotImplemented,
    #[error("Tool call id not found in request")]
    ToolCallIdNotFound,
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error(transparent)]
    BoxedError(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    McpServerError(#[from] Box<McpServerError>),
    #[error(transparent)]
    SendError(#[from] Box<tokio::sync::mpsc::error::SendError<Option<ModelEvent>>>),
    #[error("Unsupported provider: {0}")]
    UnsupportedProvider(String),
    #[error("Model stopped with error: {0}")]
    FinishError(ModelFinishError),
    #[error(transparent)]
    ModelError(#[from] Box<ModelError>),
    #[error(transparent)]
    MessageMapperError(#[from] MessageMapperError),
    #[error(transparent)]
    ValidationErrorF32(#[from] clust::ValidationError<f32>),
    #[error(transparent)]
    ValidationErrorU32(#[from] clust::ValidationError<u32>),
    #[error(transparent)]
    ValidationErrorString(#[from] clust::ValidationError<String>),
}

#[derive(Error, Debug)]
pub enum ModelFinishError {
    #[error("Content filter blocked the completion")]
    ContentFilter,

    #[error("The maximum number of tokens specified in the request was reached")]
    MaxTokens,

    #[error("Guardrail intervened and stopped this execution")]
    GuardrailIntervened,

    #[error("Tool missing content")]
    ToolMissingContent,

    #[error("Tool use doesnt have message")]
    ToolUseDoesntHaveMessage,

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("No output provided")]
    NoOutputProvided,

    #[error("No choices")]
    NoChoices,

    #[error("Content block is not in a text format. Currently only TEXT format supported")]
    ContentBlockNotInTextFormat,

    #[error("{0}")]
    Custom(String),
}

impl LLMError {
    /// Whether sending the same request again can succeed.
    ///
    /// A provider answer with an HTTP status is classified by
    /// [`llm_resilience::retryable_status`]: 408, 409, 425, 429, 5xx and 529
    /// are transient, every other 4xx (bad request, bad key, no access,
    /// context too long) fails the same way on the next attempt. Errors
    /// without a status (transport failures, stream decode errors) keep the
    /// previous behaviour and are retried.
    pub fn is_retryable(&self) -> bool {
        match self {
            LLMError::InvalidCredentials => false,
            LLMError::ModelError(e) => e.http_status().is_none_or(llm_resilience::retryable_status),
            LLMError::ReqwestError(e) => e
                .status()
                .is_none_or(|s| llm_resilience::retryable_status(s.as_u16())),
            _ => true,
        }
    }
}

/// Delay before retry number `attempt` (0-based): 250ms doubled per attempt,
/// capped at 8s, so a 429 or 529 is not re-sent back to back.
pub(crate) fn retry_backoff(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_millis(250u64.saturating_mul(1u64 << attempt.min(5)))
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    fn http(status: u16) -> LLMError {
        ModelError::ProviderHttp {
            status,
            message: String::new(),
        }
        .into()
    }

    #[test]
    fn client_errors_are_not_retried() {
        for status in [400, 401, 403, 404, 413, 422] {
            assert!(!http(status).is_retryable(), "{status} must not retry");
        }
        assert!(!LLMError::InvalidCredentials.is_retryable());
        assert!(!LLMError::from(ModelError::CredentialsError("anthropic".into())).is_retryable());
    }

    #[test]
    fn transient_errors_are_retried() {
        for status in [408, 429, 500, 502, 503, 504, 529] {
            assert!(http(status).is_retryable(), "{status} must retry");
        }
        assert!(LLMError::from(ModelError::StreamError("reset".into())).is_retryable());
    }

    #[test]
    fn bedrock_validation_is_not_retried() {
        let e: LLMError = ModelError::Bedrock(Box::new(
            crate::client::error::BedrockError::ValidationError("too long".into()),
        ))
        .into();
        assert!(!e.is_retryable());
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(retry_backoff(0).as_millis(), 250);
        assert_eq!(retry_backoff(1).as_millis(), 500);
        assert_eq!(retry_backoff(5).as_millis(), 8000);
        assert_eq!(retry_backoff(40).as_millis(), 8000);
    }
}

impl From<ModelError> for LLMError {
    fn from(value: ModelError) -> Self {
        LLMError::ModelError(Box::new(value))
    }
}

impl From<McpServerError> for LLMError {
    fn from(value: McpServerError) -> Self {
        LLMError::McpServerError(Box::new(value))
    }
}

impl From<tokio::sync::mpsc::error::SendError<Option<ModelEvent>>> for LLMError {
    fn from(value: tokio::sync::mpsc::error::SendError<Option<ModelEvent>>) -> Self {
        LLMError::SendError(Box::new(value))
    }
}

impl From<async_openai::error::OpenAIError> for LLMError {
    fn from(value: async_openai::error::OpenAIError) -> Self {
        LLMError::ModelError(Box::new(ModelError::OpenAIApi(Box::new(value))))
    }
}
