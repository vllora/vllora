use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::field;
use tracing_futures::Instrument;
use valuable::Valuable;
use vllora_llm::error::LLMError;
use vllora_llm::error::LLMResult;
use vllora_llm::types::gateway::{ChatCompletionMessage, ChatCompletionMessageWithFinishReason};
use vllora_llm::types::instance::ModelInstance;
use vllora_llm::types::message::Message;
use vllora_llm::types::{ModelEvent, ModelEventType, ModelFinishReason};
use vllora_telemetry::create_model_span;
use vllora_telemetry::events::{JsonValue, SPAN_CACHE};

macro_rules! target {
    () => {
        "vllora::user_tracing::models::cached_response"
    };
    ($subtgt:literal) => {
        concat!("vllora::user_tracing::models::cached_response::", $subtgt)
    };
}

#[derive(Debug, Clone)]
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
    ) -> LLMResult<()> {
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
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
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
            return Ok(ChatCompletionMessageWithFinishReason::new(
                response.clone(),
                ModelFinishReason::Stop,
                Uuid::new_v4().to_string(),
                chrono::Utc::now().timestamp() as u32,
                "cache".to_string(),
                None,
            ));
        }

        Err(LLMError::CustomError(
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
    ) -> LLMResult<()> {
        let span = create_model_span!(SPAN_CACHE, target!("chat"), &tags, 0, cache_state = "HIT");

        self.inner_stream(tx).instrument(span).await
    }

    async fn invoke(
        &self,
        _input_vars: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        let span = create_model_span!(SPAN_CACHE, target!("chat"), &tags, 0, cache_state = "HIT");

        self.invoke_inner(tx).instrument(span).await
    }
}
