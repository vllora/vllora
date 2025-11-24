use crate::error::GatewayError;
use crate::events::broadcast_channel_manager::BroadcastChannelManager;
use crate::events::ui_broadcaster::EventsUIBroadcaster;
use crate::types::metadata::project::Project;
use actix_web::{web, HttpResponse, Responder, Result};
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;
use vllora_llm::types::events::string_to_span_id;
use vllora_llm::types::events::CustomEventType;
use vllora_llm::types::events::Event;
use vllora_llm::types::events::EventRunContext;

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

/// Stream events endpoint handler
pub async fn stream_events(
    project: web::ReqData<Project>,
    broadcaster: web::Data<EventsUIBroadcaster>,
    project_trace_senders: web::Data<BroadcastChannelManager>,
) -> Result<impl Responder> {
    let project_slug = project.slug.clone();

    // Create a unique channel ID for this connection
    let channel_id = format!("{}:{}", project_slug, Uuid::new_v4());

    // Create a channel for sending events
    let (tx, rx) = mpsc::channel::<Event>(10000);

    tracing::debug!("New client connected to events stream: {}", channel_id);

    // Get or create the broadcast channel for this project
    let sender = project_trace_senders
        .get_or_create_channel(&project_slug)
        .map_err(|e| {
            GatewayError::CustomError(format!("Failed to create broadcast channel: {e}"))
        })?;
    let broadcast_receiver = sender.subscribe();

    // Store the sender in our application state
    broadcaster
        .add_sender(&channel_id, tx.clone(), broadcast_receiver)
        .await;

    // Spawn a cleanup task that monitors when this client disconnects
    let project_slug_cleanup = project_slug.clone();
    let project_trace_senders_cleanup = project_trace_senders.clone();
    let tx_cleanup = tx.clone();
    tokio::spawn(async move {
        // Wait for the channel to be closed (client disconnected)
        tx_cleanup.closed().await;

        tracing::debug!("Client disconnected from events stream: {}", channel_id);

        // Small delay to allow other disconnections to happen
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Try to clean up the broadcast channel if there are no more receivers
        project_trace_senders_cleanup.try_cleanup_channel(&project_slug_cleanup);
        if let Some(sender) = project_trace_senders_cleanup
            .inner()
            .get(&project_slug_cleanup)
        {
            tracing::debug!("Sender receiver count: {}", sender.receiver_count());
        }
    });

    // Create a stream for the events
    let event_stream = ReceiverStream::new(rx).map(move |event| {
        let json = serde_json::to_string(&event).unwrap_or_default();

        // Convert the event to a Server-Sent Event format
        Ok::<_, GatewayError>(web::Bytes::from(format!("data: {json}\n\n")))
    });

    // Return the response
    Ok(HttpResponse::Ok()
        .append_header(("Content-Type", "text/event-stream"))
        .append_header(("Cache-Control", "no-cache"))
        .append_header(("Connection", "keep-alive"))
        .streaming(event_stream))
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
                        .get("vllora.agent_name")
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
                        .get("vllora.task_name")
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
                        .get("vllora.tool_name")
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
