use std::collections::HashMap;

use crate::model::types::ModelEvent;
use crate::model::types::{LLMFinishEvent, ToolStartEvent};
use crate::types::gateway::ChatCompletionMessage;
use crate::GatewayError;

use crate::{
    model::ModelInstance,
    types::{
        gateway::{
            ChatCompletionChoice, ChatCompletionRequest, ChatCompletionResponse,
            ChatCompletionUsage,
        },
        threads::Message,
    },
};
use tracing::Span;
use tracing_futures::Instrument;
use uuid::Uuid;

use crate::handler::record_map_err;
use crate::GatewayApiError;

pub type FinishEventHandle =
    tokio::task::JoinHandle<(Option<LLMFinishEvent>, Option<Vec<ToolStartEvent>>)>;

#[derive(Default)]
pub struct BasicCacheContext {
    pub events_sender: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    pub response_sender: Option<tokio::sync::oneshot::Sender<ChatCompletionMessage>>,
    pub cached_events: Option<Vec<ModelEvent>>,
    pub cached_response: Option<ChatCompletionMessage>,
}

#[allow(clippy::too_many_arguments)]
pub async fn execute(
    request: ChatCompletionRequest,
    model: Box<dyn ModelInstance>,
    messages: Vec<Message>,
    tags: HashMap<String, String>,
    tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    span: Span,
    handle: Option<FinishEventHandle>,
    input_vars: HashMap<String, serde_json::Value>,
    cache_context: BasicCacheContext,
) -> Result<ChatCompletionResponse, GatewayApiError> {
    let (inner_tx, mut rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(100);
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Some(sender) = &cache_context.events_sender {
                sender.send(event.clone()).await.unwrap();
            }
            tx.send(event).await.unwrap();
        }
    });

    let response = model
        .invoke(input_vars.clone(), inner_tx, messages.clone(), tags.clone())
        .instrument(span.clone())
        .await
        .map_err(|e| record_map_err(e, span.clone()))?;

    if let Some(response_sender) = cache_context.response_sender {
        response_sender.send(response.clone()).unwrap();
    }

    let finish_reason = match (&response.tool_calls, &response.content) {
        (Some(_), _) => {
            let calls = serde_json::to_string(&response.tool_calls).unwrap();
            span.record("response", calls);
            Ok("tool_calls".to_string())
        }
        (None, Some(c)) => {
            span.record("response", c.as_string());
            Ok("stop".to_string())
        }
        _ => Err(GatewayApiError::GatewayError(GatewayError::CustomError(
            "No content in response".to_string(),
        ))),
    }?;

    let (u, _) = if let Some(handle) = handle {
        handle.await.unwrap()
    } else {
        (None, None)
    };
    let model_usage = u.and_then(|u| u.usage);
    let is_cache_used = model_usage.as_ref().map(|u| u.is_cache_used);
    let usage: ChatCompletionUsage = match model_usage {
        Some(u) => ChatCompletionUsage {
            prompt_tokens: u.input_tokens as i32,
            completion_tokens: u.output_tokens as i32,
            total_tokens: u.total_tokens as i32,
            prompt_tokens_details: u.prompt_tokens_details.clone(),
            completion_tokens_details: u.completion_tokens_details.clone(),
            cost: 0.0,
        },
        None => ChatCompletionUsage {
            ..Default::default()
        },
    };

    let response = ChatCompletionResponse {
        id: Uuid::new_v4().to_string(),
        object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: request.model.clone(),
        choices: vec![ChatCompletionChoice {
            index: 0,
            message: response.clone(),
            finish_reason: Some(finish_reason.clone()),
        }],
        usage,
        is_cache_used,
    };

    Ok(response)
}
