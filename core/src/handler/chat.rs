use std::collections::HashMap;

use crate::events::callback_handler::GatewayCallbackHandlerFn;
use crate::events::callback_handler::GatewayModelEventWithDetails;
use crate::executor::context::ExecutorContext;
use crate::history::ThreadHistoryManager;
use crate::model::types::ModelEventType;
use crate::model::DefaultModelMetadataFactory;
use crate::routing::interceptor::rate_limiter::InMemoryRateLimiterService;
use crate::routing::RoutingStrategy;
use crate::telemetry::events::JsonValue;
use crate::types::gateway::ChatCompletionRequestWithTools;
use crate::types::gateway::CompletionModelUsage;
use crate::types::gateway::Extra;
use crate::types::guardrails::service::GuardrailsEvaluator;
use crate::types::threads::ThreadEntity;
use crate::usage::InMemoryStorage;
use actix_web::{web, HttpRequest, HttpResponse};
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use valuable::Valuable;

use super::can_execute_llm_for_request;
use crate::handler::AvailableModels;
use crate::handler::CallbackHandlerFn;
use crate::history::HistoryContext;
use crate::model::ModelMetadataFactory;
use crate::types::gateway::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta, ChatCompletionUsage,
    CostCalculator,
};
use crate::types::metadata::project::Project;
use crate::types::threads::{CompletionsRunId, CompletionsThreadId};
use crate::GatewayApiError;
use tracing::Span;
use tracing_futures::Instrument;
use uuid::Uuid;

use crate::executor::chat_completion::routed_executor::RoutedExecutor;

pub type SSOChatEvent = (
    Option<ChatCompletionDelta>,
    Option<CompletionModelUsage>,
    Option<String>,
);

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(level = "debug", skip(cloud_callback_handler, history_manager))]
pub(crate) async fn prepare_request(
    cloud_callback_handler: &GatewayCallbackHandlerFn,
    tenant_name: &str,
    project_id: &str,
    identifiers: Vec<(String, String)>,
    run_id: Option<String>,
    thread_id: Option<String>,
    history_manager: Option<ThreadHistoryManager>,
    history_context: HistoryContext,
    predefined_message_id: Option<String>,
) -> Result<(JoinHandle<()>, CallbackHandlerFn), GatewayApiError> {
    let (tx, mut rx) = tokio::sync::broadcast::channel(10000);
    let callback_handler = CallbackHandlerFn(Some(tx));

    let tenant_name = tenant_name.to_string();
    let project_id = project_id.to_string();
    let identifiers = identifiers.clone();
    let cloud_callback_handler = cloud_callback_handler.clone();

    let handle = tokio::spawn(async move {
        let mut content = String::new();
        while let Ok(model_event) = rx.recv().await {
            match &model_event.event.event {
                ModelEventType::LlmContent(e) => {
                    content.push_str(&e.content);
                }
                ModelEventType::LlmStop(e) => {
                    if let Some(output) = e.output.clone() {
                        content.push_str(&output);
                    }

                    if let Some(history_manager) = &history_manager {
                        let model_name = match &model_event.model {
                            Some(model) => {
                                format!("{}/{}", model.provider_name, model.name)
                            }
                            None => e.model_name.clone(),
                        };

                        let tool_calls = e.tool_calls.iter().enumerate().map(|(index, tc)| tc.into_tool_call_with_index(index)).collect();

                        let assistant_result = history_manager
                            .insert_assistant_message(
                                content.clone(),
                                tool_calls,
                                model_name,
                                thread_id.clone(),
                                history_context.user_id.clone(),
                                &model_event.event.span,
                                run_id.clone(),
                                predefined_message_id.clone(),
                            )
                            .await;

                        if let Err(e) = assistant_result {
                            tracing::error!("Error storing assistant message: {}", e);
                        }
                    }
                }
                _ => {}
            }

            cloud_callback_handler
                .on_message(
                    GatewayModelEventWithDetails {
                        event: model_event,
                        tenant_name: tenant_name.clone(),
                        project_id: project_id.clone(),
                        usage_identifiers: identifiers.clone(),
                        run_id: run_id.clone(),
                        thread_id: thread_id.clone(),
                    }
                    .into(),
                )
                .await;
        }
    });

    Ok((handle, callback_handler))
}

#[allow(clippy::too_many_arguments)]
pub async fn create_chat_completion(
    request: web::Json<ChatCompletionRequestWithTools<RoutingStrategy>>,
    callback_handler: web::Data<GatewayCallbackHandlerFn>,
    req: HttpRequest,
    provided_models: web::Data<AvailableModels>,
    cost_calculator: web::Data<Box<dyn CostCalculator>>,
    evaluator_service: web::Data<Box<dyn GuardrailsEvaluator>>,
    thread_entity: web::Data<Box<dyn ThreadEntity>>,
    run_id: web::ReqData<CompletionsRunId>,
    thread_id: web::ReqData<CompletionsThreadId>,
    project: web::ReqData<Project>,
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
    let rate_limiter_service = InMemoryRateLimiterService::new();
    let guardrails_evaluator_service = evaluator_service.clone().into_inner();

    let history_manager = ThreadHistoryManager::new(
        thread_entity.into_inner(),
        project.settings.clone(),
        project.slug.clone(),
        &callback_handler.get_ref().clone(),
    );
    let project_slug = project.slug.clone();
    let thread_title = req
        .headers()
        .get("X-Thread-Title")
        .map(|v| v.to_str().unwrap().to_string());
    let thread_id = thread_id.value();
    // Fire-and-forget thread/message creation and insertion to run in parallel
    // with model execution. We intentionally do not await the result here.
    {
        let history_manager_clone = history_manager.clone();
        let thread_id_clone = thread_id.clone();
        let user_id_clone = "langdb".to_string();
        let project_slug_clone = project_slug.clone();
        let thread_title_clone = thread_title.clone();
        let request_clone = request.clone();
        let run_id_clone = run_id.value();
        let span_clone = span.clone();
        tokio::spawn(async move {
            let start = std::time::Instant::now();
            match history_manager_clone
                .insert_messages_for_request(
                    &thread_id_clone,
                    &user_id_clone,
                    &project_slug_clone,
                    false,
                    thread_title_clone,
                    &request_clone,
                    Some(run_id_clone),
                )
                .await
            {
                Ok(bulk_result) => {
                    tracing::info!(
                        "Messages inserted in {} ms (last_message_id={})",
                        start.elapsed().as_millis(),
                        bulk_result.last_message_id
                    );
                    span_clone.record("message_id", &bulk_result.last_message_id);
                }
                Err(e) => {
                    tracing::error!("Failed to insert messages: {}", e);
                }
            }
        });
    }

    let history_context = HistoryContext {
        thread_id: thread_id.clone(),
        user_id: "langdb".to_string(),
        model_name: request.request.model.to_string(),
    };

    let predefined_message_id = Uuid::new_v4().to_string();

    let (_handle, callback_handler_fn) = prepare_request(
        &callback_handler.get_ref().clone(),
        "langdb",
        &project_slug,
        vec![],
        Some(run_id.value()),
        Some(thread_id.clone()),
        Some(history_manager),
        history_context,
        Some(predefined_message_id),
    )
    .await?;

    let executor_context = ExecutorContext::new(
        callback_handler_fn,
        cost_calculator.into_inner(),
        Arc::new(
            Box::new(DefaultModelMetadataFactory::new(&provided_models.0))
                as Box<dyn ModelMetadataFactory>,
        ),
        &req,
        HashMap::new(),
        guardrails_evaluator_service,
        Arc::new(rate_limiter_service),
    )?;

    let executor = RoutedExecutor::new(request.clone());
    executor
        .execute(&executor_context, memory_storage, None)
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
                        prompt_tokens_details: u.prompt_tokens_details.clone(),
                        completion_tokens_details: u.completion_tokens_details.clone(),
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
