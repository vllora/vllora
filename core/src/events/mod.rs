use vllora_llm::types::events::{CustomEventType, Event};
use vllora_llm::types::ModelEventType;

use crate::events::callback_handler::GatewayEvent;

pub mod broadcast_channel_manager;
pub mod callback_handler;
pub mod ui_broadcaster;

pub fn map_cloud_event_to_agui_events(value: &GatewayEvent) -> Vec<Event> {
    match value {
        GatewayEvent::ChatEvent(event) => {
            let event_info = event.event.event.clone();
            let model = event.event.model.clone();
            match &event.event.event.event {
                ModelEventType::RunStart(_start_event) => {
                    vec![Event::RunStarted {
                        run_context: value.clone().into(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::RunEnd(_end_event) => {
                    vec![Event::RunFinished {
                        run_context: value.clone().into(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::RunError(error_event) => {
                    vec![Event::RunError {
                        run_context: value.clone().into(),
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
                            run_context: value.clone().into(),
                            timestamp,
                            custom_event: CustomEventType::SpanStart {
                                operation_name: provider_name.clone(),
                                attributes: serde_json::Value::Null,
                            },
                        },
                        Event::Custom {
                            run_context: value.clone().into(),
                            timestamp,
                            custom_event: CustomEventType::LlmStart {
                                provider_name,
                                model_name,
                                input: start_event.input.clone(),
                            },
                        },
                        Event::TextMessageStart {
                            run_context: value.clone().into(),
                            role: "assistant".to_string(),
                            timestamp,
                        },
                    ]
                }
                ModelEventType::LlmFirstToken(_) => {
                    vec![Event::StateSnapshot {
                        run_context: value.clone().into(),
                        snapshot: serde_json::json!({
                            "first_token_received": true,
                            "trace_id": event.event.event.trace_id
                        }),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::LlmContent(content_event) => {
                    vec![Event::TextMessageContent {
                        run_context: value.clone().into(),
                        delta: content_event.content.clone(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::LlmStop(stop_event) => {
                    let mut events = vec![];
                    if let Some(output) = &stop_event.output {
                        events.push(Event::TextMessageContent {
                            run_context: value.clone().into(),
                            delta: output.clone(),
                            timestamp: event_info.timestamp.timestamp_millis() as u64,
                        });
                    }
                    events.push(Event::TextMessageEnd {
                        run_context: value.clone().into(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    });
                    events.push(Event::Custom {
                        run_context: value.clone().into(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                        custom_event: CustomEventType::LlmStop {
                            content: stop_event.output.clone(),
                        },
                    });
                    events
                }
                ModelEventType::ToolStart(tool_start) => {
                    vec![Event::ToolCallStart {
                        run_context: value.clone().into(),
                        tool_call_id: tool_start.tool_id.clone(),
                        tool_call_name: tool_start.tool_name.clone(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::ToolResult(tool_result) => {
                    vec![Event::ToolCallResult {
                        run_context: value.clone().into(),
                        tool_call_id: tool_result.tool_id.clone(),
                        content: tool_result.output.clone(),
                        role: "tool".to_string(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
                ModelEventType::ImageGenerationFinish(image_event) => {
                    vec![Event::Custom {
                        run_context: value.clone().into(),
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
                        run_context: value.clone().into(),
                        custom_event: custom_event.event(),
                        timestamp: event_info.timestamp.timestamp_millis() as u64,
                    }]
                }
            }
        }
        GatewayEvent::SpanStartEvent(event) => {
            vec![Event::Custom {
                run_context: value.clone().into(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                custom_event: CustomEventType::SpanStart {
                    operation_name: event.operation_name.clone(),
                    attributes: if event.attributes.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::to_value(event.attributes.clone())
                            .expect("Failed to serialize attributes")
                    },
                },
            }]
        }
        GatewayEvent::GlobalBreakpointEvent(event) => {
            vec![Event::Custom {
                run_context: value.clone().into(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                custom_event: CustomEventType::GlobalBreakpoint {
                    intercept_all: event.intercept_all,
                },
            }]
        }
    }
}
