use crate::handler::ModelEventWithDetails;
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
        parent_span_id: Option<String>,
    ) -> Self {
        let parent_span_id = match parent_span_id {
            Some(parent_span_id) => Some(parent_span_id.clone()),
            None => {
                // Get the current span ID immediately as an owned value
                let current_span_id = {
                    let otel_context = span.context();
                    let span_ref = otel_context.span();
                    span_ref.span_context().span_id()
                };

                // Get the parent span from the current context (propagated from tracing_context middleware)
                let current = Span::current();
                let parent_context = current.context();
                let parent_span_ref = parent_context.span();
                let parent_span_context = parent_span_ref.span_context();

                // If we have a valid parent span that's different from the current span
                if parent_span_context.is_valid()
                    && parent_span_context.span_id() != current_span_id
                    && !parent_span_context.span_id().to_string().is_empty()
                {
                    Some(parent_span_context.span_id().to_string())
                } else {
                    None
                }
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

#[derive(Debug, Clone)]
pub enum GatewayEvent {
    SpanStartEvent(Box<GatewaySpanStartEvent>),
    ChatEvent(Box<GatewayModelEventWithDetails>),
}

impl GatewayEvent {
    pub fn project_id(&self) -> String {
        match self {
            GatewayEvent::SpanStartEvent(event) => event.project_id.clone(),
            GatewayEvent::ChatEvent(event) => event.project_id.clone(),
        }
    }

    pub fn tenant_name(&self) -> String {
        match self {
            GatewayEvent::SpanStartEvent(event) => event.tenant_name.clone(),
            GatewayEvent::ChatEvent(event) => event.tenant_name.clone(),
        }
    }
}

impl From<GatewayModelEventWithDetails> for GatewayEvent {
    fn from(val: GatewayModelEventWithDetails) -> Self {
        GatewayEvent::ChatEvent(Box::new(val))
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
