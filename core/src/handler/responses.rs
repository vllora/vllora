use std::{collections::HashMap, sync::Arc};

use crate::executor::responses::handle_create_response;
use crate::GatewayApiError;
use crate::{
    credentials::KeyStorage,
    events::callback_handler::{
        GatewayCallbackHandlerFn, GatewayEvent, GatewayModelEventWithDetails, GatewaySpanStartEvent,
    },
    executor::context::ExecutorContext,
    handler::{CallbackHandlerFn, ModelEventWithDetails},
    model::{DefaultModelMetadataFactory, ModelMetadataFactory},
    routing::interceptor::rate_limiter::InMemoryRateLimiterService,
    types::{
        guardrails::service::GuardrailsEvaluator,
        metadata::{project::Project, services::model::ModelService},
        threads::{CompletionsRunId, CompletionsThreadId},
    },
};
use actix_web::{web, HttpRequest, HttpResponse};
use async_openai::types::responses::{CreateResponse, Input};
use tokio::task::JoinHandle;
use tracing::Span;
use tracing_futures::Instrument;
use vllora_llm::types::{
    events::CustomEventType,
    gateway::{CostCalculator, Usage},
    CostEvent, CustomEvent, ModelEvent, ModelEventType,
};

#[allow(clippy::too_many_arguments)]
async fn prepare_request(
    cloud_callback_handler: &GatewayCallbackHandlerFn,
    request: &CreateResponse,
    tenant_name: &str,
    project_slug: &str,
    identifiers: Vec<(String, String)>,
    run_id: Option<String>,
    thread_id: Option<String>,
    cost_calculator: Arc<Box<dyn CostCalculator>>,
    span: Span,
) -> Result<(JoinHandle<()>, CallbackHandlerFn), GatewayApiError> {
    let (tx, mut rx) = tokio::sync::broadcast::channel(10000);
    let callback_handler = CallbackHandlerFn(Some(tx));

    let tenant_name = tenant_name.to_string();
    let project_slug = project_slug.to_string();
    let identifiers = identifiers.clone();
    let cloud_callback_handler = cloud_callback_handler.clone();

    let _ = cloud_callback_handler
        .on_message(GatewayEvent::SpanStartEvent(Box::new(
            GatewaySpanStartEvent::new(
                &span,
                "api_invoke".to_string(),
                project_slug.to_string(),
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

    let handle = tokio::spawn(async move {
        while let Ok(model_event) = rx.recv().await {
            let mut content = String::new();
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
                                    project_id: project_slug.clone(),
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
                        tenant_name: tenant_name.to_string(),
                        project_id: project_slug.to_string(),
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

fn get_thread_title(request: &CreateResponse) -> Option<String> {
    match &request.input {
        Input::Text(text) => Some(text.clone()),
        Input::Items(_items) => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    request: web::Json<CreateResponse>,
    cost_calculator: web::Data<Box<dyn CostCalculator>>,
    models_service: web::Data<Box<dyn ModelService>>,
    req: HttpRequest,
    evaluator_service: web::Data<Box<dyn GuardrailsEvaluator>>,
    project: web::ReqData<Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    callback_handler: web::Data<GatewayCallbackHandlerFn>,
    run_id: web::ReqData<CompletionsRunId>,
    thread_id: web::ReqData<CompletionsThreadId>,
) -> Result<HttpResponse, GatewayApiError> {
    let request = request.into_inner();

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
        || get_thread_title(&request),
        |v| Some(v.to_str().unwrap().to_string()),
    );

    if let Some(thread_title) = thread_title {
        span.record("title", thread_title);
    }

    let cost_calculator = cost_calculator.into_inner();
    let (_handle, callback_handler) = prepare_request(
        &callback_handler.get_ref().clone(),
        &request,
        "vllora",
        project.slug.as_str(),
        vec![],
        Some(run_id.value()),
        Some(thread_id.value()),
        cost_calculator.clone(),
        span.clone(),
    )
    .await?;

    let rate_limiter_service = InMemoryRateLimiterService::new();
    let guardrails_evaluator_service = evaluator_service.clone().into_inner();

    let executor_context = ExecutorContext::new(
        callback_handler,
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
        None,
    )?;

    let result = handle_create_response(&request, &executor_context)
        .instrument(span.clone())
        .await?;
    Ok(HttpResponse::Ok().json(result))
}
