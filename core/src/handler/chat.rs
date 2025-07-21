use std::collections::HashMap;

use crate::events::JsonValue;
use crate::executor::context::ExecutorContext;
use crate::routing::RoutingStrategy;
use crate::types::gateway::ChatCompletionRequestWithTools;
use crate::types::gateway::CompletionModelUsage;
use crate::types::gateway::Extra;
use crate::types::guardrails::service::GuardrailsEvaluator;
use crate::usage::InMemoryStorage;
use actix_web::{web, HttpRequest, HttpResponse};
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::Mutex;
use valuable::Valuable;

use crate::types::gateway::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta, ChatCompletionUsage,
    CostCalculator,
};
use tracing::Span;
use tracing_futures::Instrument;

use crate::handler::AvailableModels;
use crate::handler::CallbackHandlerFn;
use crate::GatewayApiError;

use super::can_execute_llm_for_request;

use crate::executor::chat_completion::routed_executor::RoutedExecutor;

pub type SSOChatEvent = (
    Option<ChatCompletionDelta>,
    Option<CompletionModelUsage>,
    Option<String>,
);

#[allow(clippy::too_many_arguments)]
pub async fn create_chat_completion(
    request: web::Json<ChatCompletionRequestWithTools<RoutingStrategy>>,
    callback_handler: web::Data<CallbackHandlerFn>,
    req: HttpRequest,
    provided_models: web::Data<AvailableModels>,
    cost_calculator: web::Data<Box<dyn CostCalculator>>,
    evaluator_service: web::Data<Box<dyn GuardrailsEvaluator>>,
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
        user = tracing::field::Empty,
    ));

    if let Some(Extra {
        user: Some(user), ..
    }) = &request.extra
    {
        span.record(
            "user",
            JsonValue(&serde_json::to_value(user.clone())?).as_value(),
        );
    }

    let memory_storage = req.app_data::<Arc<Mutex<InMemoryStorage>>>().cloned();

    let guardrails_evaluator_service = evaluator_service.clone().into_inner();
    let executor_context = ExecutorContext::new(
        callback_handler.get_ref().clone(),
        cost_calculator.into_inner(),
        provided_models.get_ref().clone(),
        &req,
        guardrails_evaluator_service,
    )?;

    let executor = RoutedExecutor::new(request.clone());
    executor
        .execute(&executor_context, memory_storage)
        .instrument(span.clone())
        .await
}

pub fn map_sso_event(
    delta: Result<SSOChatEvent, GatewayApiError>,
    model_name: String,
) -> Result<Bytes, GatewayApiError> {
    let model_name = model_name.clone();
    let chunks = match delta {
        Ok((None, usage, Some(finish_reason))) => {
            let mut chunks = vec![];
            chunks.push(ChatCompletionChunk {
                id: uuid::Uuid::new_v4().to_string(),
                object: "chat.completion.chunk".to_string(),
                created: chrono::Utc::now().timestamp(),
                model: model_name.clone(),
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: ChatCompletionDelta {
                        content: None,
                        role: None,
                        tool_calls: None,
                    },
                    finish_reason: Some(finish_reason.clone()),
                    logprobs: None,
                }],
                usage: None,
            });

            if let Some(u) = &usage {
                chunks.push(ChatCompletionChunk {
                    id: uuid::Uuid::new_v4().to_string(),
                    object: "chat.completion.chunk".to_string(),
                    created: chrono::Utc::now().timestamp(),
                    model: model_name.clone(),
                    choices: vec![],
                    usage: Some(ChatCompletionUsage {
                        prompt_tokens: u.input_tokens as i32,
                        completion_tokens: u.output_tokens as i32,
                        total_tokens: u.total_tokens as i32,
                        cost: 0.0,
                    }),
                });
            }

            Ok(chunks)
        }
        Ok((delta, _, finish_reason)) => {
            let chunk = ChatCompletionChunk {
                id: uuid::Uuid::new_v4().to_string(),
                object: "chat.completion.chunk".to_string(),
                created: chrono::Utc::now().timestamp(),
                model: model_name.clone(),
                choices: delta.as_ref().map_or(vec![], |d| {
                    vec![ChatCompletionChunkChoice {
                        index: 0,
                        delta: d.clone(),
                        finish_reason,
                        logprobs: None,
                    }]
                }),
                usage: None,
            };

            Ok(vec![chunk])
        }
        Err(e) => Err(e),
    };

    let mut result_combined = String::new();
    match chunks {
        Ok(chunks) => {
            for c in chunks {
                let json_str = serde_json::to_string(&c).unwrap_or_else(|e| {
                    format!("{{\"error\": \"Failed to serialize chunk: {e}\"}}")
                });

                result_combined.push_str(&format!("data: {json_str}\n\n"));
            }
        }
        Err(e) => {
            let result = serde_json::to_string(&HashMap::from([("error", e.to_string())]))
                .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize chunk: {e}\"}}"));

            result_combined.push_str(&format!("data: {result}\n\n"));
        }
    }

    Ok(Bytes::from(result_combined))
}
