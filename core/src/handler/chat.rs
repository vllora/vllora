use std::collections::HashMap;

use crate::types::gateway::ChatCompletionRequestWithTools;
use crate::types::gateway::CompletionModelUsage;
use crate::usage::InMemoryStorage;
use actix_web::{web, HttpRequest, HttpResponse};
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::types::gateway::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta, ChatCompletionUsage,
    CostCalculator,
};
use tracing::Span;
use tracing_futures::Instrument;

use crate::handler::AvailableModels;
use crate::handler::CallbackHandlerFn;
use crate::otel::TraceMap;
use crate::GatewayApiError;

use super::can_execute_llm_for_request;

use crate::executor::chat_completion::routed_executor::RoutedExecutor;

#[allow(clippy::too_many_arguments)]
pub async fn create_chat_completion(
    request: web::Json<ChatCompletionRequestWithTools>,
    callback_handler: web::Data<CallbackHandlerFn>,
    traces: web::Data<TraceMap>,
    req: HttpRequest,
    provided_models: web::Data<AvailableModels>,
    cost_calculator: web::Data<Box<dyn CostCalculator>>,
) -> Result<HttpResponse, GatewayApiError> {
    can_execute_llm_for_request(&req).await?;

    let span = Span::or_current(tracing::info_span!(
        target: "langdb::user_tracing::api_invoke",
        "api_invoke",
        request = tracing::field::Empty,
        response = tracing::field::Empty,
        error = tracing::field::Empty,
        thread_id = tracing::field::Empty,
        message_id = tracing::field::Empty,
    ));

    let memory_storage = req.app_data::<Arc<Mutex<InMemoryStorage>>>().cloned();

    let executor = RoutedExecutor::new(request.clone());
    executor
        .execute(
            callback_handler.get_ref(),
            traces.get_ref(),
            &req,
            provided_models.get_ref(),
            cost_calculator.into_inner(),
            memory_storage.as_ref(),
        )
        .instrument(span.clone())
        .await
}

pub fn map_sso_event(
    delta: Result<(Option<ChatCompletionDelta>, Option<CompletionModelUsage>), GatewayApiError>,
    model_name: String,
) -> Result<Bytes, GatewayApiError> {
    let model_name = model_name.clone();
    let chunk = match delta {
        Ok((None, None)) => Ok(None),
        Ok((delta, usage)) => {
            let chunk = ChatCompletionChunk {
                id: uuid::Uuid::new_v4().to_string(),
                object: "chat.completion.chunk".to_string(),
                created: chrono::Utc::now().timestamp(),
                model: model_name.clone(),
                choices: delta.as_ref().map_or(vec![], |d| {
                    vec![ChatCompletionChunkChoice {
                        index: 0,
                        delta: d.clone(),
                        finish_reason: None,
                        logprobs: None,
                    }]
                }),
                usage: usage.as_ref().map(|u| ChatCompletionUsage {
                    prompt_tokens: u.input_tokens as i32,
                    completion_tokens: u.output_tokens as i32,
                    total_tokens: u.total_tokens as i32,
                    cost: 0.0,
                }),
            };

            Ok(Some(chunk))
        }
        Err(e) => Err(e),
    };

    let json_str = match chunk {
        Ok(r) => r.map(|c| {
            serde_json::to_string(&c)
                .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize chunk: {}\"}}", e))
        }),
        Err(e) => Some(
            serde_json::to_string(&HashMap::from([("error", e.to_string())]))
                .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize chunk: {}\"}}", e)),
        ),
    };

    let result = json_str
        .as_ref()
        .map_or(String::new(), |json_str| format!("data: {json_str}\n\n"));

    Ok(Bytes::from(result))
}
