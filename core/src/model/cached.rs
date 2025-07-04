use crate::error::GatewayError;
use crate::events::{JsonValue, SPAN_CACHE};
use crate::model::types::{ModelEvent, ModelEventType};
use crate::model::ModelInstance;
use crate::types::gateway::ChatCompletionMessage;
use crate::types::threads::Message;
use crate::GatewayResult;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::field;
use tracing_futures::Instrument;
use valuable::Valuable;

macro_rules! target {
    () => {
        "langdb::user_tracing::models::cached_response"
    };
    ($subtgt:literal) => {
        concat!("langdb::user_tracing::models::cached_response::", $subtgt)
    };
}

#[derive(Debug)]
pub struct CachedModel {
    events: Vec<ModelEvent>,
    response: Option<ChatCompletionMessage>,
}

impl CachedModel {
    pub fn new(events: Vec<ModelEvent>, response: Option<ChatCompletionMessage>) -> Self {
        Self { events, response }
    }

    async fn inner_stream(
        &self,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    ) -> GatewayResult<()> {
        for event in &self.events {
            if let ModelEventType::LlmStop(e) = &event.event {
                let mut u = e.usage.clone();
                if let Some(u) = u.as_mut() {
                    u.is_cache_used = true;
                }
                let mut event_type = e.clone();
                event_type.usage = u;

                let mut ev = event.clone();
                ev.event = ModelEventType::LlmStop(event_type);
                tx.send(Some(ev)).await?;
                continue;
            }
            tx.send(Some(event.clone())).await?;
        }
        tx.send(None).await?;
        Ok(())
    }

    async fn invoke_inner(
        &self,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    ) -> GatewayResult<ChatCompletionMessage> {
        tracing::warn!("Cached model invoke");

        for event in &self.events {
            if let ModelEventType::LlmStop(e) = &event.event {
                let mut u = e.usage.clone();
                if let Some(u) = u.as_mut() {
                    u.is_cache_used = true;
                }
                let mut event_type = e.clone();
                event_type.usage = u;

                let mut ev = event.clone();
                ev.event = ModelEventType::LlmStop(event_type);
                tx.send(Some(ev)).await?;
                continue;
            }

            tx.send(Some(event.clone())).await?;
        }
        tx.send(None).await?;

        if let Some(response) = &self.response {
            return Ok(response.clone());
        }

        Err(GatewayError::CustomError(
            "Cached model response is None".to_string(),
        ))
    }
}

#[async_trait]
impl ModelInstance for CachedModel {
    async fn stream(
        &self,
        _input_vars: HashMap<String, Value>,
        tx: mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        let span = tracing::info_span!(
            target: target!("chat"),
            SPAN_CACHE,
            cache_state = "HIT",
            output = field::Empty,
            error = field::Empty,
            usage = field::Empty,
            ttft = field::Empty,
            tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value(),
            request = field::Empty
        );

        self.inner_stream(tx).instrument(span).await
    }

    async fn invoke(
        &self,
        _input_vars: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessage> {
        let span = tracing::info_span!(
            target: target!("chat"),
            SPAN_CACHE,
            cache_state = "HIT",
            output = field::Empty,
            error = field::Empty,
            usage = field::Empty,
            ttft = field::Empty,
            tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value(),
            request = field::Empty
        );

        self.invoke_inner(tx).instrument(span).await
    }
}
