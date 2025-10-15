use crate::events::callback_handler::GatewayEvent;
use crate::events::callback_handler::GatewayModelEventWithDetails;
use crate::model::types::CostEvent;
use crate::model::types::ModelEventType;
use serde::{Deserialize, Serialize};

pub mod broadcast_channel_manager;
pub mod callback_handler;
pub mod ui_broadcaster;

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
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
}

impl From<&GatewayModelEventWithDetails> for EventRunContext {
    fn from(value: &GatewayModelEventWithDetails) -> Self {
        EventRunContext {
            run_id: value.run_id.clone(),
            thread_id: value.thread_id.clone(),
            span_id: None,
            parent_span_id: None,
        }
    }
}

impl From<&GatewayEvent> for EventRunContext {
    fn from(value: &GatewayEvent) -> Self {
        match value {
            GatewayEvent::SpanStartEvent(event) => EventRunContext {
                run_id: event.run_id.clone(),
                thread_id: event.thread_id.clone(),
                span_id: Some(event.span_id.to_string()),
                parent_span_id: event.parent_span_id.clone(),
            },
            GatewayEvent::ChatEvent(event) => EventRunContext {
                run_id: event.run_id.clone(),
                thread_id: event.thread_id.clone(),
                span_id: Some(event.event.event.span_id.to_string()),
                parent_span_id: event.event.event.parent_span_id.clone(),
            },
        }
    }
}

pub fn map_cloud_event_to_agui_events(value: &GatewayEvent) -> Vec<Event> {
    match value {
        GatewayEvent::ChatEvent(event) => {
            let event_info = event.event.event.clone();
            let model = event.event.model.clone();
            match &event.event.event.event {
                ModelEventType::RunStart(_start_event) => {
                    vec![Event::RunStarted {
                        run_context: value.into(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::RunEnd(_end_event) => {
                    vec![Event::RunFinished {
                        run_context: value.into(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::RunError(error_event) => {
                    vec![Event::RunError {
                        run_context: value.into(),
                        message: error_event.message.clone(),
                        code: error_event.code.clone(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::LlmStart(start_event) => {
                    let timestamp = event_info.timestamp.timestamp_millis() as u64;
                    let (provider_name, model_name) = match model {
                        Some(model) => (model.provider_name, model.name),
                        None => (
                            start_event.provider_name.clone(),
                            start_event.model_name.clone(),
                        ),
                    };

                    vec![
                        Event::Custom {
                            run_context: value.into(),
                            timestamp,
                            custom_event: CustomEventType::LlmStart {
                                provider_name,
                                model_name,
                                input: start_event.input.clone(),
                            },
                        },
                        Event::TextMessageStart {
                            run_context: value.into(),
                            role: "assistant".to_string(),
                            timestamp,
                        },
                        // Event::Custom {
                        //     run_context: value.into(),
                        //     name: "model_start".to_string(),
                        //     value: model_value,
                        //     timestamp,
                        // },
                    ]
                }
                ModelEventType::LlmFirstToken(_) => {
                    vec![Event::StateSnapshot {
                        run_context: value.into(),
                        snapshot: serde_json::json!({
                            "first_token_received": true,
                            "trace_id": event.event.event.trace_id
                        }),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::LlmContent(content_event) => {
                    vec![Event::TextMessageContent {
                        run_context: value.into(),
                        delta: content_event.content.clone(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::LlmStop(stop_event) => {
                    let mut events = vec![];
                    if let Some(output) = &stop_event.output {
                        events.push(Event::TextMessageContent {
                            run_context: value.into(),
                            delta: output.clone(),
                            timestamp: event_info.timestamp.timestamp_millis() as u64,
                        });
                    }
                    events.push(Event::TextMessageEnd {
                        run_context: value.into(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    });
                    events.push(Event::Custom {
                        run_context: value.into(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                        custom_event: CustomEventType::LlmStop {
                            content: stop_event.output.clone(),
                        },
                    });
                    events
                }
                ModelEventType::ToolStart(tool_start) => {
                    vec![Event::ToolCallStart {
                        run_context: value.into(),
                        tool_call_id: tool_start.tool_id.clone(),
                        tool_call_name: tool_start.tool_name.clone(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::ToolResult(tool_result) => {
                    vec![Event::ToolCallResult {
                        run_context: value.into(),
                        tool_call_id: tool_result.tool_id.clone(),
                        content: tool_result.output.clone(),
                        role: "tool".to_string(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::ImageGenerationFinish(image_event) => {
                    vec![Event::Custom {
                        run_context: value.into(),
                        custom_event: CustomEventType::ImageGenerationFinish {
                            model_name: image_event.model_name.clone(),
                            quality: image_event.quality.clone(),
                            size: image_event.size.to_string(),
                            count_of_images: image_event.count_of_images,
                            steps: image_event.steps,
                        },
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::Custom(custom_event) => {
                    vec![Event::Custom {
                        run_context: value.into(),
                        custom_event: custom_event.event(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
            }
        }
        GatewayEvent::SpanStartEvent(event) => {
            vec![Event::Custom {
                run_context: value.into(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                custom_event: CustomEventType::SpanStart {
                    operation_name: event.operation_name.clone(),
                    attributes: serde_json::Value::Null,
                },
            }]
        }
    }
}
