use crate::handler::chat::map_sso_event;
use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::executor::chat_completion::execute;
use crate::routing::RouteStrategy;
use crate::types::gateway::ChatCompletionRequestWithTools;
use crate::usage::InMemoryStorage;
use crate::GatewayError;
use actix_web::{HttpRequest, HttpResponse};
use bytes::Bytes;
use either::Either::{Left, Right};
use futures::StreamExt;
use futures::TryStreamExt;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::types::gateway::CostCalculator;
use opentelemetry::trace::TraceContextExt as _;
use tokio::sync::broadcast;
use tracing::Span;
use tracing_futures::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use crate::handler::find_model_by_full_name;
use crate::handler::AvailableModels;
use crate::handler::CallbackHandlerFn;
use crate::otel::{trace_id_uuid, TraceMap};
use crate::routing::LlmRouter;
use crate::GatewayApiError;

use crate::events::JsonValue;
use crate::events::SPAN_REQUEST_ROUTING;
use tracing::field;
use valuable::Valuable;

#[derive(Error, Debug)]
pub enum RoutedExecutorError {
    #[error("Failed deserializing request to json: {0}")]
    FailedToDeserializeRequestResult(#[from] serde_json::Error),

    #[error("Failed serializing merged request with target: {0}")]
    FailedToSerializeMergedRequestResult(serde_json::Error),
}

pub struct RoutedExecutor {
    request: ChatCompletionRequestWithTools,
}

impl RoutedExecutor {
    pub fn new(request: ChatCompletionRequestWithTools) -> Self {
        Self { request }
    }

    pub async fn execute(
        &self,
        callback_handler: &CallbackHandlerFn,
        traces: &TraceMap,
        req: &HttpRequest,
        available_models: &AvailableModels,
        cost_calculator: Arc<Box<dyn CostCalculator>>,
        memory_storage: Option<&Arc<Mutex<InMemoryStorage>>>,
    ) -> Result<HttpResponse, GatewayApiError> {
        let span = Span::current();

        let mut targets = vec![(self.request.clone(), None)];

        while let Some((mut request, target)) = targets.pop() {
            if let Some(t) = target {
                request.router = None;
                request = Self::merge_request_with_target(&request, &t)?;
            }

            if let Some(router) = &request.router {
                let router_name = request
                    .request
                    .model
                    .split('/')
                    .last()
                    .expect("Model name should not be empty")
                    .to_string();
                span.record("router_name", &router_name);

                let span = tracing::info_span!(
                    target: "langdb::user_tracing::request_routing",
                    SPAN_REQUEST_ROUTING,
                    router_name = router_name,
                    before = JsonValue(&serde_json::to_value(&request.request)?).as_value(),
                    after = field::Empty
                );

                let llm_router = LlmRouter {
                    name: router.name.clone().unwrap_or("dynamic".to_string()),
                    strategy: router.strategy.clone(),
                    targets: router.targets.clone(),
                };

                let metrics = match memory_storage {
                    Some(storage) => {
                        let guard = storage.lock().await;
                        guard.get_all_counters().await
                    }
                    None => BTreeMap::new(),
                };

                let executor_result = llm_router
                    .route(
                        request.request.clone(),
                        available_models,
                        req.headers()
                            .into_iter()
                            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                            .collect(),
                        metrics,
                    )
                    .instrument(span.clone())
                    .await;

                match executor_result {
                    Ok(executor_result) => {
                        for t in executor_result.iter().rev() {
                            targets.push((request.clone(), Some(t.clone())));
                        }
                    }
                    Err(e) => {
                        tracing::error!("Router error: {}, route ignored", e);
                    }
                }
            } else {
                let result = Self::execute_request(
                    &request,
                    callback_handler,
                    traces,
                    req,
                    available_models,
                    cost_calculator.clone(),
                )
                .await;

                match result {
                    Ok(response) => return Ok(response),
                    Err(err) => {
                        if targets.is_empty() {
                            return Err(err);
                        } else {
                            tracing::warn!(
                                "Error executing request: {:?}, so moving to next target",
                                err
                            );
                        }
                    }
                }
            }
        }

        unreachable!()
    }

    async fn execute_request(
        request: &ChatCompletionRequestWithTools,
        callback_handler: &CallbackHandlerFn,
        traces: &TraceMap,
        req: &HttpRequest,
        available_models: &AvailableModels,
        cost_calculator: Arc<Box<dyn CostCalculator>>,
    ) -> Result<HttpResponse, GatewayApiError> {
        let span = tracing::Span::current();
        span.record("request", &serde_json::to_string(&request)?);
        let trace_id = span.context().span().span_context().trace_id();
        traces
            .entry(trace_id)
            .or_insert_with(|| broadcast::channel(8));

        let model_name = request.request.model.clone();

        let llm_model = find_model_by_full_name(&request.request.model, available_models)?;

        let response = execute(
            request,
            callback_handler,
            req.clone(),
            cost_calculator,
            &llm_model,
        )
        .instrument(span.clone())
        .await?;

        let mut response_builder = HttpResponse::Ok();
        let builder = response_builder
            .insert_header(("X-Trace-Id", trace_id_uuid(trace_id).to_string()))
            .insert_header(("X-Model-Name", model_name.clone()))
            .insert_header((
                "X-Provider-Name",
                llm_model.inference_provider.provider.to_string(),
            ));

        match response {
            Left(result_stream) => {
                let stream = result_stream?.map_err(|e| {
                    GatewayApiError::GatewayError(GatewayError::CustomError(e.to_string()))
                });

                // Pin the stream to heap
                let mut stream = Box::pin(stream);

                // Check first element for error
                let first = match stream.as_mut().next().await {
                    Some(Ok(delta)) => delta,
                    Some(Err(e)) => {
                        return Err(e);
                    }
                    None => {
                        return Err(GatewayApiError::GatewayError(GatewayError::CustomError(
                            "Empty response from model".to_string(),
                        )));
                    }
                };

                let model_name = model_name.clone();
                let result = futures::stream::once(async { Ok(first) })
                    .chain(stream)
                    .then(move |delta| {
                        let model_name = model_name.clone();
                        async move { map_sso_event(delta, model_name) }
                    })
                    .chain(futures::stream::once(async {
                        Ok::<_, GatewayApiError>(Bytes::from("data: [DONE]\n\n"))
                    }));

                Ok(builder.content_type("text/event-stream").streaming(result))
            }
            Right(completions_response) => Ok(builder.json(completions_response?)),
        }
    }

    fn merge_request_with_target(
        request: &ChatCompletionRequestWithTools,
        target: &HashMap<String, serde_json::Value>,
    ) -> Result<ChatCompletionRequestWithTools, RoutedExecutorError> {
        let mut request_value = serde_json::to_value(request)
            .map_err(RoutedExecutorError::FailedToDeserializeRequestResult)?;

        if let Some(obj) = request_value.as_object_mut() {
            for (key, value) in target {
                // Only override if the new value is not null
                if !value.is_null() {
                    obj.insert(key.clone(), value.clone());
                }
            }
        }

        serde_json::from_value(request_value)
            .map_err(RoutedExecutorError::FailedToDeserializeRequestResult)
    }
}
