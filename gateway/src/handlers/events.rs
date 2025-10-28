use actix_web::{web, HttpResponse, Result};
use langdb_core::events::string_to_span_id;
use langdb_core::events::ui_broadcaster::EventsUIBroadcaster;
use langdb_core::events::CustomEventType;
use langdb_core::events::{Event, EventRunContext};
use langdb_core::types::metadata::project::Project;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEvent {
    pub span_id: String,
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub operation: Operation,
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Run,
    Agent,
    Task,
    Tool,
    Other(String),
}

impl<'de> Deserialize<'de> for Operation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "run" => Operation::Run,
            "agent" => Operation::Agent,
            "task" => Operation::Task,
            "tool" => Operation::Tool,
            other => Operation::Other(other.to_string()),
        })
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum SendEventsRequest {
    Single(CustomEvent),
    Multiple(Vec<CustomEvent>),
}

#[derive(Serialize)]
pub struct SendEventsResponse {
    pub success: bool,
    pub message: String,
    pub events_sent: usize,
}

/// Send events to EventsUIBroadcaster for the current project
pub async fn send_events(
    project: web::ReqData<Project>,
    broadcaster: web::Data<EventsUIBroadcaster>,
    req: web::Json<SendEventsRequest>,
) -> Result<HttpResponse> {
    let project = project.into_inner();

    let custom_events = match req.into_inner() {
        SendEventsRequest::Single(event) => vec![event],
        SendEventsRequest::Multiple(events) => events,
    };

    let event_count = custom_events.len();

    // Convert custom events to Event::Custom format for the broadcaster
    let events: Vec<Event> = custom_events
        .into_iter()
        .map(|custom_event| {
            let run_id = custom_event
                .attributes
                .get("vllora.run_id")
                .unwrap_or_default()
                .as_str()
                .map(|s| s.to_string());
            let thread_id = custom_event
                .attributes
                .get("vllora.thread_id")
                .unwrap_or_default()
                .as_str()
                .map(|s| s.to_string());

            let span_id = string_to_span_id(&custom_event.span_id);
            let parent_span_id = custom_event
                .parent_span_id
                .and_then(|s| string_to_span_id(&s));
            let run_context = EventRunContext {
                run_id,
                thread_id,
                span_id,
                parent_span_id,
            };
            match custom_event.operation {
                Operation::Run => Event::RunStarted {
                    run_context,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                },
                Operation::Agent => {
                    let name = custom_event
                        .attributes
                        .get("langdb.agent_name")
                        .unwrap_or_default()
                        .as_str()
                        .map(|s| s.to_string());
                    Event::AgentStarted {
                        run_context,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        name,
                    }
                }
                Operation::Task => {
                    let name = custom_event
                        .attributes
                        .get("langdb.task_name")
                        .unwrap_or_default()
                        .as_str()
                        .map(|s| s.to_string());
                    Event::TaskStarted {
                        run_context,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        name,
                    }
                }
                Operation::Tool => {
                    let tool_name = custom_event
                        .attributes
                        .get("langdb.tool_name")
                        .unwrap_or_default()
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    Event::ToolCallStart {
                        run_context,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        tool_call_name: tool_name,
                        tool_call_id: custom_event
                            .attributes
                            .get("langdb.tool_call_id")
                            .unwrap_or_default()
                            .as_str()
                            .map(|s| s.to_string())
                            .unwrap_or_default(),
                    }
                }
                Operation::Other(operation) => Event::Custom {
                    run_context,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    custom_event: CustomEventType::CustomEvent {
                        operation: operation.clone(),
                        attributes: custom_event.attributes,
                    },
                },
            }
        })
        .collect();

    // Send events to the broadcaster for this project
    broadcaster
        .send_events(&project.slug.to_string(), &events)
        .await;

    let response = SendEventsResponse {
        success: true,
        message: format!(
            "Successfully sent {} event(s) to project {}",
            event_count, project.id
        ),
        events_sent: event_count,
    };

    Ok(HttpResponse::Ok().json(response))
}
