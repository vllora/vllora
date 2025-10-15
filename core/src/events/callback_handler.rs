use crate::handler::ModelEventWithDetails;
use crate::types::message::MessageType;
use crate::types::threads::MessageThreadWithTitle;
use opentelemetry::trace::TraceContextExt;
use serde::Serialize;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::events::ui_broadcaster::EventsUIBroadcaster;

#[derive(Debug, Clone)]
pub struct GatewayModelEventWithDetails {
    pub event: ModelEventWithDetails,
    pub tenant_name: String,
    pub project_id: String,
    pub usage_identifiers: Vec<(String, String)>,
    pub run_id: Option<String>,
    pub thread_id: Option<String>,
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayThreadEvent {
    #[serde(rename = "event_type")]
    pub event_type: ThreadEventType,
    #[serde(flatten)]
    pub thread: MessageThreadWithTitle,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudMessageEvent {
    #[serde(rename = "event_type")]
    pub event_type: MessageEventType,
    pub thread_id: String,
    pub message_id: String,
    pub message_type: MessageType,
    pub project_id: String,
    pub tenant_name: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewaySpanStartEvent {
    pub project_id: String,
    pub tenant_name: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub trace_id: String,
    pub run_id: Option<String>,
    pub thread_id: Option<String>,
    pub operation_name: String,
}

impl GatewaySpanStartEvent {
    pub fn new(
        span: &Span,
        operation_name: String,
        project_id: String,
        tenant_name: String,
        run_id: Option<String>,
        thread_id: Option<String>,
    ) -> Self {
        let parent_span_id = {
            let current = Span::current();
            let current_span_id = current.context().span().span_context().span_id();
            let this_span_id = span.context().span().span_context().span_id();

            // If the passed span is different from current, current is likely the parent
            if current_span_id != this_span_id && !current_span_id.to_string().is_empty() {
                Some(current_span_id.to_string())
            } else {
                None
            }
        };

        Self {
            span_id: span.context().span().span_context().span_id().to_string(),
            parent_span_id,
            trace_id: span.context().span().span_context().trace_id().to_string(),
            operation_name,
            project_id,
            tenant_name,
            run_id,
            thread_id,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadEventType {
    Created,
    Updated,
    Deleted,
    MessageCreated,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageEventType {
    Created,
}

#[derive(Debug, Clone)]
pub enum GatewayEvent {
    SpanStartEvent(Box<GatewaySpanStartEvent>),
    ChatEvent(Box<GatewayModelEventWithDetails>),
    ThreadEvent(Box<GatewayThreadEvent>),
    MessageEvent(Box<CloudMessageEvent>),
}

impl GatewayEvent {
    pub fn project_id(&self) -> String {
        match self {
            GatewayEvent::SpanStartEvent(event) => event.project_id.clone(),
            GatewayEvent::ChatEvent(event) => event.project_id.clone(),
            GatewayEvent::ThreadEvent(event) => event.thread.project_id.clone(),
            GatewayEvent::MessageEvent(event) => event.project_id.clone(),
        }
    }

    pub fn tenant_name(&self) -> String {
        match self {
            GatewayEvent::SpanStartEvent(event) => event.tenant_name.clone(),
            GatewayEvent::ChatEvent(event) => event.tenant_name.clone(),
            GatewayEvent::ThreadEvent(_event) => "".to_string(),
            GatewayEvent::MessageEvent(event) => event.tenant_name.clone(),
        }
    }
}

impl From<GatewayModelEventWithDetails> for GatewayEvent {
    fn from(val: GatewayModelEventWithDetails) -> Self {
        GatewayEvent::ChatEvent(Box::new(val))
    }
}

impl From<GatewayThreadEvent> for GatewayEvent {
    fn from(val: GatewayThreadEvent) -> Self {
        GatewayEvent::ThreadEvent(Box::new(val))
    }
}

impl From<CloudMessageEvent> for GatewayEvent {
    fn from(val: CloudMessageEvent) -> Self {
        GatewayEvent::MessageEvent(Box::new(val))
    }
}

impl From<GatewaySpanStartEvent> for GatewayEvent {
    fn from(val: GatewaySpanStartEvent) -> Self {
        GatewayEvent::SpanStartEvent(Box::new(val))
    }
}

#[derive(Clone, Default)]
pub struct GatewayCallbackHandlerFn {
    senders: Vec<tokio::sync::broadcast::Sender<GatewayEvent>>,
    ui_broadcaster: Option<EventsUIBroadcaster>,
}

impl GatewayCallbackHandlerFn {
    pub fn new(
        senders: Vec<tokio::sync::broadcast::Sender<GatewayEvent>>,
        ui_broadcaster: Option<EventsUIBroadcaster>,
    ) -> Self {
        Self {
            senders,
            ui_broadcaster,
        }
    }

    pub async fn on_message(&self, message: GatewayEvent) {
        for sender in self.senders.clone() {
            let _ = sender.send(message.clone());
        }

        if let Some(broadcaster) = &self.ui_broadcaster {
            broadcaster.broadcast_event(&message).await;
        }
    }
}
