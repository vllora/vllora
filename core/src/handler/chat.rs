use std::collections::HashMap;

use crate::events::callback_handler::GatewayCallbackHandlerFn;
use crate::events::callback_handler::GatewayEvent;
use crate::events::callback_handler::GatewayModelEventWithDetails;
use crate::events::callback_handler::GatewaySpanStartEvent;
use crate::executor::context::ExecutorContext;
use crate::handler::ModelEventWithDetails;
use crate::metadata::pool::DbPool;
use crate::metadata::services::project::ProjectServiceImpl;
use crate::model::DefaultModelMetadataFactory;
use crate::routing::interceptor::rate_limiter::InMemoryRateLimiterService;
use crate::routing::RoutingStrategy;
use crate::types::guardrails::service::GuardrailsEvaluator;
use crate::types::metadata::services::project::ProjectService;
use crate::usage::InMemoryStorage;
use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use valuable::Valuable;
use vllora_llm::types::events::CustomEventType;
use vllora_llm::types::gateway::ChatCompletionRequestWithTools;
use vllora_llm::types::gateway::Extra;
use vllora_llm::types::gateway::GatewayModelUsage;
use vllora_llm::types::gateway::Usage;
use vllora_llm::types::CostEvent;
use vllora_llm::types::CustomEvent;
use vllora_llm::types::ModelEvent;
use vllora_llm::types::ModelEventType;
use vllora_telemetry::events::JsonValue;

use super::can_execute_llm_for_request;
use crate::handler::CallbackHandlerFn;
use crate::model::ModelMetadataFactory;
use crate::types::metadata::project::Project;
use crate::types::metadata::services::model::ModelService;
use crate::types::threads::{CompletionsRunId, CompletionsThreadId};
use crate::GatewayApiError;
use tracing::Span;
use tracing_futures::Instrument;
use vllora_llm::types::gateway::{ChatCompletionDelta, CostCalculator};

use crate::credentials::KeyStorage;
use crate::executor::chat_completion::breakpoint::BreakpointManager;
use crate::executor::chat_completion::routed_executor::RoutedExecutor;

pub type SSOChatEvent = (
    Option<ChatCompletionDelta>,
    Option<GatewayModelUsage>,
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
    request: &ChatCompletionRequestWithTools<RoutingStrategy>,
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
                Some(HashMap::from([(
                    "request".to_string(),
                    serde_json::to_value(request.clone())?,
                )])),
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
                                (0.0, None)
                            }
                        };

                        let cost_event = CostEvent::new(cost, usage.clone());
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
                            span.record("usage", serde_json::to_string(&usage).unwrap());
                            span.record("response", content.clone());
                        }

                        span.record("cost", serde_json::to_string(&cost).unwrap());
                        span.record("usage", serde_json::to_string(&usage).unwrap());
                        span.record("response", content.clone());
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
    breakpoint_manager: web::Data<BreakpointManager>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, GatewayApiError> {
    can_execute_llm_for_request(&req).await?;

    let span = Span::or_current(tracing::info_span!(
        target: "vllora::user_tracing::api_invoke",
        "api_invoke",
        request = tracing::field::Empty,
        response = tracing::field::Empty,
        error = tracing::field::Empty,
        thread_id = tracing::field::Empty,
        message_id = tracing::field::Empty,
        user = tracing::field::Empty,
        title = tracing::field::Empty,
        cost = tracing::field::Empty,
        usage = tracing::field::Empty,
    ));

    let thread_title = req.headers().get("X-Thread-Title").map_or_else(
        || {
            let message = request.request.messages.iter().find(|m| m.role == "user");
            match message {
                Some(message) => message.content.as_ref().and_then(|c| c.as_string()),
                None => None,
            }
        },
        |v| Some(v.to_str().unwrap().to_string()),
    );

    if let Some(thread_title) = thread_title {
        span.record("title", thread_title);
    }

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

    let thread_id = thread_id.value();

    let cost_calculator = cost_calculator.into_inner();
    let request = request.into_inner();
    let (_handle, callback_handler_fn) = prepare_request(
        &callback_handler.get_ref().clone(),
        "vllora",
        &project.slug,
        vec![],
        Some(run_id.value()),
        Some(thread_id.clone()),
        span.clone(),
        cost_calculator.clone(),
        &request,
    )
    .await?;

    // If the project is Lucy, we need to use the default project for execution
    let project_id = if project.slug == "lucy" {
        let project_service = ProjectServiceImpl::new(db_pool.get_ref().clone());
        let project = project_service
            .get_default(project.company_id)
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

        project.id
    } else {
        project.id
    };

    let db_pool = db_pool.into_inner();
    let executor_context = ExecutorContext::new(
        callback_handler_fn,
        cost_calculator,
        Arc::new(Box::new(
            DefaultModelMetadataFactory::new(models_service.into_inner()).with_db_pool(&db_pool),
        ) as Box<dyn ModelMetadataFactory>),
        &req,
        HashMap::new(),
        guardrails_evaluator_service,
        Arc::new(rate_limiter_service),
        project_id,
        key_storage.into_inner(),
        None,
    )?;

    let executor = RoutedExecutor::new(request.clone());
    executor
        .execute(
            &executor_context,
            memory_storage,
            None,
            Some(&thread_id),
            Some(&breakpoint_manager.into_inner()),
        )
        .instrument(span.clone())
        .await
        .inspect_err(|e| {
            span.record("error", e.to_string());
        })
}
