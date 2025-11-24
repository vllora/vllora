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
