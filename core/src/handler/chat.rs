use std::collections::HashMap;

use crate::events::callback_handler::GatewayCallbackHandlerFn;
use crate::events::callback_handler::GatewayEvent;
use crate::events::callback_handler::GatewayModelEventWithDetails;
use crate::events::callback_handler::GatewaySpanStartEvent;
use crate::events::CustomEventType;
use crate::executor::context::ExecutorContext;
use crate::handler::ModelEventWithDetails;
use crate::model::types::CostEvent;
use crate::model::types::CustomEvent;
use crate::model::types::ModelEvent;
use crate::model::types::ModelEventType;
use crate::model::DefaultModelMetadataFactory;
use crate::routing::interceptor::rate_limiter::InMemoryRateLimiterService;
use crate::routing::RoutingStrategy;
use crate::telemetry::events::JsonValue;
use crate::types::gateway::ChatCompletionRequestWithTools;
use crate::types::gateway::CompletionModelUsage;
use crate::types::gateway::Extra;
use crate::types::gateway::Usage;
use crate::types::guardrails::service::GuardrailsEvaluator;
use crate::usage::InMemoryStorage;
use actix_web::{web, HttpRequest, HttpResponse};
use bytes::Bytes;
use opentelemetry::trace::TraceContextExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use valuable::Valuable;

use super::can_execute_llm_for_request;
use crate::handler::CallbackHandlerFn;
use crate::metadata::services::model::ModelService;
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

use crate::credentials::KeyStorage;
use crate::executor::chat_completion::routed_executor::RoutedExecutor;

pub type SSOChatEvent = (
    Option<ChatCompletionDelta>,
    Option<CompletionModelUsage>,
    Option<String>,
);

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(level = "debug", skip(cloud_callback_handler, span, cost_calculator))]
pub(crate) async fn prepare_request(
    cloud_callback_handler: &GatewayCallbackHandlerFn,
    tenant_name: &str,
    project_id: &str,
    identifiers: Vec<(String, String)>,
    run_id: Option<String>,
    thread_id: Option<String>,
    span: Span,
    cost_calculator: Arc<Box<dyn CostCalculator>>,
) -> Result<(JoinHandle<()>, CallbackHandlerFn), GatewayApiError> {
    let (tx, mut rx) = tokio::sync::broadcast::channel(10000);
    let callback_handler = CallbackHandlerFn(Some(tx));

    let tenant_name = tenant_name.to_string();
    let project_id = project_id.to_string();
    let identifiers = identifiers.clone();
    let cloud_callback_handler = cloud_callback_handler.clone();

    let _ = cloud_callback_handler
        .on_message(GatewayEvent::SpanStartEvent(Box::new(
            GatewaySpanStartEvent::new(
                &span,
                "api_invoke".to_string(),
                project_id.to_string(),
                tenant_name.to_string(),
                run_id.clone(),
                thread_id.clone(),
                None,
            ),
        )))
        .await;

    let span = span.clone();
    let cost_calculator = cost_calculator.clone();
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

                    if let Some(model) = &model_event.model {
                        let (cost, usage) = match &e.usage {
                            Some(usage) => {
                                let cost = cost_calculator
                                    .as_ref()
                                    .calculate_cost(
                                        &model.price,
                                        &Usage::CompletionModelUsage(usage.clone()),
                                        &model.credentials_ident,
                                    )
                                    .await
                                    .map(|c| (c.cost, Some(usage.clone())))
                                    .unwrap_or((0.0, Some(usage.clone())));

                                if cost.0 == 0.0 {
                                    tracing::error!(
                                        "Cost is 0 for event {e:?}. Event: {event:?}",
                                        event = model_event.event
                                    );
                                }

                                cost
                            }
                            None => {
                                tracing::info!(
                                    "Usage is none for event {:?}. Event: {:?}",
                                    e,
                                    model_event.event
                                );
                                tracing::error!(
                                    "Usage is none for event {e:?}. Event: {event:?}",
                                    event = model_event.event
                                );
                                (0.0, None)
                            }
                        };

                        let cost_event = CostEvent::new(cost, usage);
                        let _ = cloud_callback_handler
                            .on_message(
                                GatewayModelEventWithDetails {
                                    event: ModelEventWithDetails::new(
                                        ModelEvent::new(
                                            &span,
                                            ModelEventType::Custom(CustomEvent::new(
                                                CustomEventType::Cost {
                                                    value: cost_event.clone(),
                                                },
                                            )),
                                        ),
                                        model_event.model.clone(),
                                    ),
                                    tenant_name: tenant_name.clone(),
                                    project_id: project_id.clone(),
                                    usage_identifiers: identifiers.clone(),
                                    run_id: run_id.clone(),
                                    thread_id: thread_id.clone(),
                                }
                                .into(),
                            )
                            .await;

                        if let Some(span) = &model_event.event.span {
                            span.record("cost", serde_json::to_string(&cost).unwrap());
                        }
                        span.record("cost", serde_json::to_string(&cost).unwrap());
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
    cost_calculator: web::Data<Box<dyn CostCalculator>>,
    evaluator_service: web::Data<Box<dyn GuardrailsEvaluator>>,
    run_id: web::ReqData<CompletionsRunId>,
    thread_id: web::ReqData<CompletionsThreadId>,
    project: web::ReqData<Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    models_service: web::Data<Box<dyn ModelService>>,
    context: web::ReqData<opentelemetry::Context>,
) -> Result<HttpResponse, GatewayApiError> {
    let context_span = context.span();
    let span_context = context_span.span_context();
    let _ = callback_handler
        .on_message(GatewayEvent::SpanStartEvent(Box::new(
            GatewaySpanStartEvent::new(
                &Span::current(),
                "cloud_api_invoke".to_string(),
                project.slug.to_string(),
                "default".to_string(),
                Some(run_id.value()),
                Some(thread_id.value()),
                if span_context.is_valid() {
                    Some(span_context.span_id().to_string())
                } else {
                    None
                },
            ),
        )))
        .await;

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

    let project_slug = project.slug.clone();
    // let thread_title = req
    //     .headers()
    //     .get("X-Thread-Title")
    //     .map(|v| v.to_str().unwrap().to_string());
    let thread_id = thread_id.value();

    let cost_calculator = cost_calculator.into_inner();
    let (_handle, callback_handler_fn) = prepare_request(
        &callback_handler.get_ref().clone(),
        "langdb",
        &project_slug,
        vec![],
        Some(run_id.value()),
        Some(thread_id.clone()),
        span.clone(),
        cost_calculator.clone(),
    )
    .await?;

    let executor_context = ExecutorContext::new(
        callback_handler_fn,
        cost_calculator,
        Arc::new(Box::new(DefaultModelMetadataFactory::new(
            models_service.into_inner(),
        )) as Box<dyn ModelMetadataFactory>),
        &req,
        HashMap::new(),
        guardrails_evaluator_service,
        Arc::new(rate_limiter_service),
        project.id,
        key_storage.into_inner(),
    )?;

    let executor = RoutedExecutor::new(request.clone());
    executor
        .execute(&executor_context, memory_storage, None, Some(&thread_id))
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
