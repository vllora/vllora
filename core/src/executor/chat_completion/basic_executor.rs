use std::collections::HashMap;

use crate::model::types::ModelEvent;
use crate::model::types::{LLMFinishEvent, ToolStartEvent};
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

pub async fn execute(
    request: ChatCompletionRequest,
    model: Box<dyn ModelInstance>,
    messages: Vec<Message>,
    tags: HashMap<String, String>,
    tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    span: Span,
    handle: tokio::task::JoinHandle<(Option<LLMFinishEvent>, Option<Vec<ToolStartEvent>>)>,
) -> Result<ChatCompletionResponse, GatewayApiError> {
    let result = model
        .invoke(HashMap::new(), tx, messages.clone(), tags.clone())
        .instrument(span.clone())
        .await
        .map_err(|e| record_map_err(e, span.clone()))?;

    let finish_reason = match (&result.tool_calls, &result.content) {
        (Some(_), _) => {
            let calls = serde_json::to_string(&result.tool_calls).unwrap();
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

    let (u, _) = handle.await.unwrap();
    let model_usage = u.and_then(|u| u.usage);
    let usage: ChatCompletionUsage = match model_usage {
        Some(u) => ChatCompletionUsage {
            prompt_tokens: u.input_tokens as i32,
            completion_tokens: u.output_tokens as i32,
            total_tokens: u.total_tokens as i32,
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
            message: result.clone(),
            finish_reason: Some(finish_reason.clone()),
        }],
        usage,
    };

    Ok(response)
}
