use opentelemetry::SpanId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::types::{gateway::ChatCompletionRequest, CostEvent};

/// Events based on the Agent User Interaction Protocol
/// See https://docs.ag-ui.com/concepts/events for details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    // Lifecycle Events
    RunStarted {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    RunFinished {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    RunError {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Error message
        message: String,
        /// Optional error code
        code: Option<String>,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    AgentStarted {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Timestamp of when the event occurred
        timestamp: u64,
        name: Option<String>,
    },
    AgentFinished {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    TaskStarted {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Timestamp of when the event occurred
        timestamp: u64,
        name: Option<String>,
    },
    TaskFinished {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    StepStarted {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Name of the step being started
        step_name: String,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    StepFinished {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Name of the step being finished (must match a previous StepStarted event)
        step_name: String,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },

    // Text Message Events
    TextMessageStart {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Role (e.g., "assistant", "user")
        role: String,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    TextMessageContent {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Incremental content to append to the message
        delta: String,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    TextMessageEnd {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },

    // Tool Call Events
    ToolCallStart {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Unique identifier for this tool call
        tool_call_id: String,
        /// Name of the tool being called
        tool_call_name: String,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    ToolCallArgs {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Incremental arguments data
        delta: String,
        /// Associated tool call identifier
        tool_call_id: String,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    ToolCallEnd {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Associated tool call identifier
        tool_call_id: String,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    ToolCallResult {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Associated tool call identifier
        tool_call_id: String,
        /// Result content
        content: String,
        /// Role (typically "tool")
        role: String,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },

    // State Management Events
    StateSnapshot {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Complete state snapshot as a JSON value
        snapshot: serde_json::Value,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    StateDelta {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// State delta as JSON Patch operations
        delta: serde_json::Value,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    MessagesSnapshot {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Complete messages history
        messages: Vec<serde_json::Value>,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },

    // Special Events
    Raw {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Original event data
        event: serde_json::Value,
        /// Optional source system identifier
        source: Option<String>,
        /// Timestamp of when the event occurred
        timestamp: u64,
    },
    Custom {
        #[serde(flatten)]
        run_context: EventRunContext,
        /// Timestamp of when the event occurred
        timestamp: u64,
        #[serde(rename = "event")]
        custom_event: CustomEventType,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CustomEventType {
    SpanStart {
        operation_name: String,
        attributes: serde_json::Value,
    },
    SpanEnd {
        operation_name: String,
        attributes: serde_json::Value,
        start_time_unix_nano: u64,
        finish_time_unix_nano: u64,
    },
    Ping,
    ImageGenerationFinish {
        model_name: String,
        quality: String,
        size: String,
        count_of_images: u8,
        steps: u8,
    },
    LlmStart {
        provider_name: String,
        model_name: String,
        input: String,
    },
    LlmStop {
        content: Option<String>,
    },
    Cost {
        value: CostEvent,
    },
    CustomEvent {
        operation: String,
        attributes: serde_json::Value,
    },
    Breakpoint {
        request: Box<ChatCompletionRequest>,
    },
    BreakpointResume {
        updated_request: Option<Box<ChatCompletionRequest>>,
    },
}

impl Event {
    /// Get the timestamp of the event
    pub fn timestamp(&self) -> u64 {
        match self {
            Event::RunStarted { timestamp, .. } => *timestamp,
            Event::RunFinished { timestamp, .. } => *timestamp,
            Event::RunError { timestamp, .. } => *timestamp,
            Event::StepStarted { timestamp, .. } => *timestamp,
            Event::StepFinished { timestamp, .. } => *timestamp,
            Event::AgentStarted { timestamp, .. } => *timestamp,
            Event::AgentFinished { timestamp, .. } => *timestamp,
            Event::TaskStarted { timestamp, .. } => *timestamp,
            Event::TaskFinished { timestamp, .. } => *timestamp,
            Event::TextMessageStart { timestamp, .. } => *timestamp,
            Event::TextMessageContent { timestamp, .. } => *timestamp,
            Event::TextMessageEnd { timestamp, .. } => *timestamp,
            Event::ToolCallStart { timestamp, .. } => *timestamp,
            Event::ToolCallArgs { timestamp, .. } => *timestamp,
            Event::ToolCallEnd { timestamp, .. } => *timestamp,
            Event::ToolCallResult { timestamp, .. } => *timestamp,
            Event::StateSnapshot { timestamp, .. } => *timestamp,
            Event::StateDelta { timestamp, .. } => *timestamp,
            Event::MessagesSnapshot { timestamp, .. } => *timestamp,
            Event::Raw { timestamp, .. } => *timestamp,
            Event::Custom { timestamp, .. } => *timestamp,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRunContext {
    pub run_id: Option<String>,
    pub thread_id: Option<String>,
    #[serde(
        serialize_with = "serialize_option_span_id",
        deserialize_with = "deserialize_option_span_id"
    )]
    pub span_id: Option<SpanId>,
    #[serde(
        serialize_with = "serialize_option_span_id",
        deserialize_with = "deserialize_option_span_id"
    )]
    pub parent_span_id: Option<SpanId>,
}

pub fn serialize_span_id<S>(span_id: &SpanId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&u64::from_be_bytes(span_id.to_bytes()).to_string())
}

pub fn serialize_option_span_id<S>(
    span_id: &Option<SpanId>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match span_id {
        Some(span_id) => serialize_span_id(span_id, serializer),
        None => serializer.serialize_none(),
    }
}

fn deserialize_option_span_id<'de, D>(deserializer: D) -> Result<Option<SpanId>, D::Error>
where
    D: Deserializer<'de>,
{
    let span_id = Option::<String>::deserialize(deserializer)?;
    let span_id = match span_id {
        Some(s) if s == "0" => None,
        Some(s) => Some(SpanId::from_hex(&s).unwrap()),
        None => None,
    };
    Ok(span_id)
}

pub fn string_to_span_id(span_id: &str) -> Option<SpanId> {
    SpanId::from_hex(span_id).ok()
}
