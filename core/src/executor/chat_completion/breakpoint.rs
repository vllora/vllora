use opentelemetry::SpanId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tracing::Span;
use vllora_llm::types::events::Event;
use vllora_llm::types::{
    events::CustomEventType, gateway::ChatCompletionRequest, CustomEvent, ModelEvent,
    ModelEventType,
};

use crate::handler::{CallbackHandlerFn, ModelEventWithDetails};

/// Action to take when continuing from a breakpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "serde", untagged, rename_all = "snake_case")]
pub enum BreakpointAction {
    /// Continue with the original request
    Continue,
    /// Continue with a modified request
    ModifyRequest(Box<ChatCompletionRequest>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestWithThreadId {
    pub request: ChatCompletionRequest,
    pub thread_id: Option<String>,
}

/// Manager for handling breakpoints across requests
#[derive(Clone)]
pub struct BreakpointManager {
    pending_breakpoints: Arc<Mutex<HashMap<String, oneshot::Sender<BreakpointAction>>>>,
    breakpoint_requests: Arc<Mutex<HashMap<String, RequestWithThreadId>>>,
    intercept_all: Arc<AtomicBool>,
    events_storage: Arc<Mutex<HashMap<String, Vec<Event>>>>,
}

impl BreakpointManager {
    pub fn new() -> Self {
        Self {
            pending_breakpoints: Arc::new(Mutex::new(HashMap::new())),
            breakpoint_requests: Arc::new(Mutex::new(HashMap::new())),
            intercept_all: Arc::new(AtomicBool::new(false)),
            events_storage: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Enable or disable intercepting all requests regardless of tags
    pub async fn set_intercept_all(&self, value: bool) {
        self.intercept_all.store(value, Ordering::Relaxed);
        if !value {
            self.continue_all().await;
        }
    }

    /// Returns whether all requests should be intercepted regardless of tags
    pub fn intercept_all(&self) -> bool {
        self.intercept_all.load(Ordering::Relaxed)
    }

    /// Continue all pending breakpoints with the original request
    pub async fn continue_all(&self) {
        let mut pending = self.pending_breakpoints.lock().await;
        let mut requests = self.breakpoint_requests.lock().await;

        // Drain all pending breakpoints and send Continue to each
        for (breakpoint_id, sender) in pending.drain() {
            // Remove stored request as we're resuming execution
            let request_with_thread_id = requests.remove(&breakpoint_id);
            if let Some(request_with_thread_id) = request_with_thread_id {
                self.clear_events_for_request(&request_with_thread_id).await;
            }
            if let Err(_action) = sender.send(BreakpointAction::Continue) {
                tracing::error!(
                    breakpoint_id = %breakpoint_id,
                    "Failed to continue breakpoint: receiver dropped"
                );
            }
        }
    }

    pub async fn clear_events_for_request(&self, request_with_thread_id: &RequestWithThreadId) {
        if let Some(thread_id) = &request_with_thread_id.thread_id {
            self.clear_events_by_thread_id(thread_id).await;
        }
    }

    /// Register a breakpoint and return a receiver to wait for the action
    pub async fn register_breakpoint(
        &self,
        breakpoint_id: String,
        request: ChatCompletionRequest,
        thread_id: Option<&String>,
    ) -> oneshot::Receiver<BreakpointAction> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending_breakpoints.lock().await;
        let mut requests = self.breakpoint_requests.lock().await;
        pending.insert(breakpoint_id.clone(), tx);
        requests.insert(
            breakpoint_id,
            RequestWithThreadId {
                request,
                thread_id: thread_id.cloned(),
            },
        );
        rx
    }

    /// Resolve a breakpoint with the given action
    pub async fn resolve_breakpoint(
        &self,
        breakpoint_id: &str,
        action: BreakpointAction,
    ) -> Result<(), BreakpointError> {
        let mut pending = self.pending_breakpoints.lock().await;
        let mut requests = self.breakpoint_requests.lock().await;
        if let Some(tx) = pending.remove(breakpoint_id) {
            // remove stored request when resolved
            let request_with_thread_id = requests.remove(breakpoint_id);
            if let Some(request_with_thread_id) = request_with_thread_id {
                self.clear_events_for_request(&request_with_thread_id).await;
            }
            tx.send(action)
                .map_err(|_| BreakpointError::ChannelClosed)?;
            Ok(())
        } else {
            Err(BreakpointError::BreakpointNotFound(
                breakpoint_id.to_string(),
            ))
        }
    }

    /// Check if a breakpoint exists
    pub async fn has_breakpoint(&self, breakpoint_id: &str) -> bool {
        let pending = self.pending_breakpoints.lock().await;
        pending.contains_key(breakpoint_id)
    }

    /// List all currently pending breakpoints and their stored requests
    pub async fn list_breakpoints(&self) -> Vec<(String, RequestWithThreadId)> {
        let pending = self.pending_breakpoints.lock().await;
        let requests = self.breakpoint_requests.lock().await;

        pending
            .keys()
            .filter_map(|id| requests.get(id).cloned().map(|req| (id.clone(), req)))
            .collect()
    }

    /// Store events grouped by thread_id
    pub async fn store_events(&self, events: &[Event]) {
        if !self.intercept_all() {
            return;
        }
        let mut storage = self.events_storage.lock().await;
        for event in events {
            if let Some(thread_id) = event.thread_id() {
                storage
                    .entry(thread_id.clone())
                    .or_insert_with(Vec::new)
                    .push(event.clone());
            }
        }
    }

    /// Retrieve all events for a specific thread_id
    pub async fn get_events_by_thread_id(&self, thread_id: &str) -> Vec<Event> {
        let storage = self.events_storage.lock().await;
        storage.get(thread_id).cloned().unwrap_or_default()
    }

    /// Clear events for a specific thread_id
    pub async fn clear_events_by_thread_id(&self, thread_id: &str) {
        let mut storage = self.events_storage.lock().await;
        storage.remove(thread_id);
    }
}

impl Default for BreakpointManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BreakpointError {
    #[error("Breakpoint not found: {0}")]
    BreakpointNotFound(String),
    #[error("Channel closed")]
    ChannelClosed,
}

/// Wait for breakpoint action if debug tag is present
pub async fn wait_for_breakpoint_action(
    executor_tags: &HashMap<String, String>,
    breakpoint_manager: &BreakpointManager,
    request: &ChatCompletionRequest,
    callback_handler: &CallbackHandlerFn,
    thread_id: Option<&String>,
) -> Result<ChatCompletionRequest, BreakpointError> {
    // If global intercept is disabled, only intercept when "debug" tag is present
    if !breakpoint_manager.intercept_all() && !executor_tags.contains_key("debug") {
        return Ok(request.clone());
    }

    let span = Span::current();
    let event = ModelEventWithDetails::new(
        ModelEvent::new(
            &span,
            ModelEventType::Custom(CustomEvent::new(CustomEventType::Breakpoint {
                request: Box::new(request.clone()),
            })),
        ),
        None,
    );
    let breakpoint_id = SpanId::from_hex(&event.event.span_id)
        .ok()
        .map(|id| u64::from_be_bytes(id.to_bytes()).to_string())
        .unwrap_or_default();
    callback_handler.on_message(event);

    // Register the breakpoint and get the receiver
    let rx = breakpoint_manager
        .register_breakpoint(breakpoint_id.clone(), request.clone(), thread_id)
        .await;

    // Log that we're waiting for breakpoint
    tracing::info!(
        breakpoint_id = %breakpoint_id,
        "Waiting for breakpoint action. Use POST /debug/continue with breakpoint_id to continue."
    );

    // Wait for the action
    match rx.await {
        Ok(action) => {
            let span = Span::current();
            let event = ModelEventWithDetails::new(
                ModelEvent::new(
                    &span,
                    ModelEventType::Custom(CustomEvent::new(CustomEventType::BreakpointResume {
                        updated_request: match &action {
                            BreakpointAction::Continue => None,
                            BreakpointAction::ModifyRequest(modified_request) => {
                                Some(modified_request.clone())
                            }
                        },
                    })),
                ),
                None,
            );
            callback_handler.on_message(event);
            match action {
                BreakpointAction::Continue => {
                    tracing::info!(breakpoint_id = %breakpoint_id, "Breakpoint: Continuing with original request");
                    Ok(request.clone())
                }
                BreakpointAction::ModifyRequest(modified_request) => {
                    tracing::info!(breakpoint_id = %breakpoint_id, "Breakpoint: Continuing with modified request");
                    Ok(*modified_request)
                }
            }
        }
        Err(_) => {
            tracing::error!(breakpoint_id = %breakpoint_id, "Breakpoint channel closed unexpectedly");
            Err(BreakpointError::ChannelClosed)
        }
    }
}
