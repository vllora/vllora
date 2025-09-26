use crate::types::gateway::{CompletionModelUsage, ImageSize};
use chrono::{DateTime, Utc};
use opentelemetry::trace::TraceContextExt;
use serde::{Deserialize, Serialize};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::CredentialsIdent;

#[derive(Debug, Serialize, Deserialize)]
pub enum StreamEvent {
    Text(String),
    Error(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomEvent {
    value: serde_json::Value,
    name: String,
}

impl CustomEvent {
    pub fn new(name: String, value: serde_json::Value) -> Self {
        Self { value, name }
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn value(&self) -> serde_json::Value {
        self.value.clone()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type", content = "data")]
pub enum ModelEventType {
    RunStart(RunStartEvent),
    RunEnd(RunEndEvent),
    RunError(RunErrorEvent),
    LlmStart(LLMStartEvent),
    LlmFirstToken(LLMFirstToken),
    LlmContent(LLMContentEvent),
    LlmStop(LLMFinishEvent),
    ToolStart(ToolStartEvent),
    ToolResult(ToolResultEvent),
    ImageGenerationFinish(ImageGenerationFinishEvent),
    Custom(CustomEvent),
}
impl ModelEventType {
    pub fn as_str(&self) -> &str {
        match self {
            ModelEventType::RunStart(_) => "run_start",
            ModelEventType::RunEnd(_) => "run_end",
            ModelEventType::RunError(_) => "run_error",
            ModelEventType::LlmStart(_) => "llm_start",
            ModelEventType::LlmContent(_) => "llm_content",
            ModelEventType::LlmStop(_) => "llm_stop",
            ModelEventType::ToolStart(_) => "tool_start",
            ModelEventType::ToolResult(_) => "tool_result",
            ModelEventType::ImageGenerationFinish(_) => "image_generation_finish",
            ModelEventType::LlmFirstToken(_) => "llm_first_token",
            ModelEventType::Custom(_) => "custom",
        }
    }
}
#[derive(Debug, Serialize, Deserialize, Clone)]

pub struct ModelEvent {
    pub span_id: String,
    pub trace_id: String,
    pub event: ModelEventType,
    pub timestamp: DateTime<Utc>,
    #[serde(skip)]
    pub span: Option<Span>,
}

impl ModelEvent {
    pub fn new(span: &Span, event_type: ModelEventType) -> Self {
        Self {
            event: event_type,
            timestamp: Utc::now(),
            span_id: span.context().span().span_context().span_id().to_string(),
            trace_id: span.context().span().span_context().trace_id().to_string(),
            span: Some(span.clone()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LLMContentEvent {
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]

pub struct LLMStartEvent {
    pub provider_name: String,
    pub model_name: String,
    pub input: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]

pub struct LLMFirstToken {}

#[derive(Debug, Serialize, Deserialize, Clone)]

pub struct LLMFinishEvent {
    pub provider_name: String,
    pub model_name: String,
    pub output: Option<String>,
    pub usage: Option<CompletionModelUsage>,
    pub finish_reason: ModelFinishReason,
    pub tool_calls: Vec<ModelToolCall>,
    pub credentials_ident: CredentialsIdent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelToolCall {
    pub tool_id: String,
    pub tool_name: String,
    pub input: String,
}
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelFinishReason {
    Stop,
    StopSequence,
    Length,
    ToolCalls,
    ContentFilter,
    Guardrail,
    Other(String),
}

impl std::fmt::Display for ModelFinishReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelFinishReason::Stop => write!(f, "stop"),
            ModelFinishReason::StopSequence => write!(f, "stop_sequence"),
            ModelFinishReason::Length => write!(f, "length"),
            ModelFinishReason::ToolCalls => write!(f, "tool_calls"),
            ModelFinishReason::ContentFilter => write!(f, "content_filter"),
            ModelFinishReason::Guardrail => write!(f, "guardrail"),
            ModelFinishReason::Other(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolStartEvent {
    pub tool_id: String,
    pub tool_name: String,
    pub input: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolResultEvent {
    pub tool_id: String,
    pub tool_name: String,
    pub is_error: bool,
    pub output: String,
}

pub struct ModelToolResult {
    pub tool_id: String,
    pub tool_name: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageGenerationFinishEvent {
    pub model_name: String,
    pub quality: String,
    pub size: ImageSize,
    pub count_of_images: u8,
    pub steps: u8,
    pub credentials_ident: CredentialsIdent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunStartEvent {
    pub run_id: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunEndEvent {
    pub run_id: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunErrorEvent {
    pub run_id: String,
    pub thread_id: Option<String>,
    pub message: String,
    pub code: Option<String>,
}
