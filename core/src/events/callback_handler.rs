use crate::handler::ModelEventWithDetails;
use crate::types::message::MessageType;
use crate::types::threads::MessageThreadWithTitle;
use serde::Serialize;

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
    ChatEvent(Box<GatewayModelEventWithDetails>),
    ThreadEvent(Box<GatewayThreadEvent>),
    MessageEvent(Box<CloudMessageEvent>),
}

impl GatewayEvent {
    pub fn project_id(&self) -> String {
        match self {
            GatewayEvent::ChatEvent(event) => event.project_id.clone(),
            GatewayEvent::ThreadEvent(event) => event.thread.project_id.clone(),
            GatewayEvent::MessageEvent(event) => event.project_id.clone(),
        }
    }

    pub fn tenant_name(&self) -> String {
        match self {
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
