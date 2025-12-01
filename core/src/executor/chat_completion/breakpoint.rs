use opentelemetry::SpanId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tracing::Span;
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

/// Manager for handling breakpoints across requests
#[derive(Clone)]
pub struct BreakpointManager {
    pending_breakpoints: Arc<Mutex<HashMap<String, oneshot::Sender<BreakpointAction>>>>,
}

impl BreakpointManager {
    pub fn new() -> Self {
        Self {
            pending_breakpoints: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a breakpoint and return a receiver to wait for the action
    pub async fn register_breakpoint(
        &self,
        breakpoint_id: String,
    ) -> oneshot::Receiver<BreakpointAction> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending_breakpoints.lock().await;
        pending.insert(breakpoint_id, tx);
        rx
    }

    /// Resolve a breakpoint with the given action
    pub async fn resolve_breakpoint(
        &self,
        breakpoint_id: &str,
        action: BreakpointAction,
    ) -> Result<(), BreakpointError> {
        let mut pending = self.pending_breakpoints.lock().await;
        if let Some(tx) = pending.remove(breakpoint_id) {
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
) -> Result<ChatCompletionRequest, BreakpointError> {
    // Check if "debug" tag is present
    if !executor_tags.contains_key("debug") {
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
        .register_breakpoint(breakpoint_id.clone())
        .await;

    // Log that we're waiting for breakpoint
    tracing::info!(
        breakpoint_id = %breakpoint_id,
        "Waiting for breakpoint action. Use POST /debug/continue with breakpoint_id to continue."
    );

    // Wait for the action
    match rx.await {
        Ok(action) => match action {
            BreakpointAction::Continue => {
                tracing::info!(breakpoint_id = %breakpoint_id, "Breakpoint: Continuing with original request");
                Ok(request.clone())
            }
            BreakpointAction::ModifyRequest(modified_request) => {
                tracing::info!(breakpoint_id = %breakpoint_id, "Breakpoint: Continuing with modified request");
                Ok(*modified_request)
            }
        },
        Err(_) => {
            tracing::error!(breakpoint_id = %breakpoint_id, "Breakpoint channel closed unexpectedly");
            Err(BreakpointError::ChannelClosed)
        }
    }
}
