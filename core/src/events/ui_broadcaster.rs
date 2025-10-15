use crate::events::callback_handler::GatewayEvent;
use crate::events::{map_cloud_event_to_agui_events, Event};
use crate::events::{CustomEventType, EventRunContext};
use crate::telemetry::Span;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::sync::{broadcast, Mutex};

#[derive(Clone)]
pub struct EventsUIBroadcaster {
    pub senders_container: Arc<EventsSendersContainer>,
}

impl EventsUIBroadcaster {
    pub fn new(senders_container: Arc<EventsSendersContainer>) -> Self {
        Self { senders_container }
    }

    pub async fn add_sender(
        &self,
        channel_id: &str,
        sender: Sender<Event>,
        mut traces_receiver: broadcast::Receiver<Span>,
    ) {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let mut senders = self.senders_container.senders.lock().await;
        senders.insert(channel_id.to_string(), (sender.clone(), Some(shutdown_tx)));

        let container = self.senders_container.clone();
        let sender_inner = sender.clone();
        let channel_id = channel_id.to_string();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    span = traces_receiver.recv() => {
                        match span {
                            Ok(span) => {
                                if span.operation_name == "run" {
                                    let _ =sender.send(Event::RunFinished {
                                        run_context: EventRunContext {
                                            run_id: span.run_id,
                                            thread_id: span.thread_id,
                                            span_id: Some(span.span_id.to_string()),
                                            parent_span_id: span.parent_span_id.map(|id| id.to_string()),
                                        },
                                        timestamp: span.end_time_unix_nano,
                                    }).await;
                                } else if span.operation_name == "agent" {
                                    let _ = sender.send(Event::AgentFinished {
                                        run_context: EventRunContext {
                                            run_id: span.run_id,
                                            thread_id: span.thread_id,
                                            span_id: Some(span.span_id.to_string()),
                                            parent_span_id: span.parent_span_id.map(|id| id.to_string()),
                                        },
                                        timestamp: span.end_time_unix_nano,
                                    }).await;
                                } else if span.operation_name == "task" {
                                    let _ = sender.send(Event::TaskFinished {
                                        run_context: EventRunContext {
                                            run_id: span.run_id,
                                            thread_id: span.thread_id,
                                            span_id: Some(span.span_id.to_string()),
                                            parent_span_id: span.parent_span_id.map(|id| id.to_string()),
                                        },
                                        timestamp: span.end_time_unix_nano,
                                    }).await;
                                } else {
                                    let _ = sender.send(Event::Custom {
                                        custom_event: CustomEventType::SpanEnd {
                                            operation_name: span.operation_name,
                                            attributes: span.attributes.into(),
                                            start_time_unix_nano: span.start_time_unix_nano,
                                            finish_time_unix_nano: span.end_time_unix_nano,
                                        },
                                        timestamp: std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as u64,
                                        run_context: EventRunContext {
                                            run_id: span.run_id,
                                            thread_id: span.thread_id,
                                            span_id: Some(span.span_id.to_string()),
                                            parent_span_id: span.parent_span_id.map(|id| id.to_string()),
                                        },
                                    }).await;
                                }
                            }
                            Err(_) => break, // channel closed
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break; // received shutdown signal
                    }
                }
            }
        });

        tokio::spawn(async move {
            loop {
                match sender_inner
                    .send(Event::Custom {
                        custom_event: CustomEventType::Ping,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        run_context: EventRunContext {
                            run_id: None,
                            thread_id: None,
                            span_id: None,
                            parent_span_id: None,
                        },
                    })
                    .await
                {
                    Ok(_) => {}
                    Err(_) => break,
                }

                // Sleep for 5 seconds
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }

            let mut senders = container.senders.lock().await;
            if let Some((_, Some(shutdown))) = senders.remove(&channel_id) {
                let _ = shutdown.send(());
            }
        });
    }

    pub async fn broadcast_event(&self, event: &GatewayEvent) {
        let events = map_cloud_event_to_agui_events(event);
        self.send_events(&event.project_id(), &events).await;
    }

    /// Helper function to send an event to all connected clients for a specific project
    pub async fn send_events(&self, project_id: &str, events: &[Event]) {
        // Lock the senders map
        let mut senders = self.senders_container.senders.lock().await;

        // Find all senders for this project and send the event
        let mut channels_to_remove = vec![];
        for (channel_id, (tx, _)) in senders.iter() {
            if channel_id.starts_with(project_id) {
                for event in events {
                    // Ignore errors as they likely mean the client disconnected
                    match tx.send(event.clone()).await {
                        Ok(_) => {}
                        Err(_) => {
                            channels_to_remove.push(channel_id.clone());
                            break;
                        }
                    }
                }
            }
        }

        for ch in channels_to_remove {
            if let Some((_, Some(shutdown))) = senders.remove(&ch) {
                let _ = shutdown.send(());
            }
        }
    }
}

#[derive(Clone)]
pub struct EventsSendersContainer {
    pub senders: EventSenders,
}

impl EventsSendersContainer {
    pub fn new(senders: EventSenders) -> Self {
        Self { senders }
    }
}
/// Type alias for the map of event senders
type EventSenders = Arc<Mutex<HashMap<String, (Sender<Event>, Option<oneshot::Sender<()>>)>>>;
