use crate::executor::chat_completion::basic_executor::BasicCacheContext;
use crate::executor::context::ExecutorContext;
use crate::routing::metrics::InMemoryMetricsRepository;
use crate::routing::RoutingStrategy;
use crate::usage::InMemoryStorage;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use vllora_llm::types::credentials_ident::CredentialsIdent;
use vllora_llm::types::gateway::ChatCompletionChunk;
use vllora_llm::types::gateway::CompletionModelUsage;
use vllora_llm::types::gateway::Usage;
use vllora_llm::types::models::InferenceProvider;
use vllora_llm::types::models::ModelMetadata;
use vllora_llm::types::provider::InferenceModelProvider;

use crate::executor::chat_completion::execute;
use crate::routing::RouteStrategy;
use vllora_llm::types::gateway::ChatCompletionRequestWithTools;

use crate::GatewayError;
use actix_web::HttpResponse;
use bytes::Bytes;
use either::Either::{Left, Right};
use futures::StreamExt;

use crate::executor::chat_completion::StreamCacheContext;
use thiserror::Error;

use opentelemetry::trace::TraceContextExt as _;
use tokio::sync::Mutex;
use tracing::Span;
use tracing_futures::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use crate::routing::LlmRouter;
use crate::GatewayApiError;
use vllora_telemetry::trace_id_uuid;

use tracing::field;
use valuable::Valuable;
use vllora_telemetry::events::JsonValue;
use vllora_telemetry::events::SPAN_REQUEST_ROUTING;

const MAX_DEPTH: usize = 10;

#[derive(Error, Debug)]
pub enum RoutedExecutorError {
    #[error("Failed deserializing request to json: {0}")]
    FailedToDeserializeRequestResult(#[from] serde_json::Error),

    #[error("Failed serializing merged request with target: {0}")]
    FailedToSerializeMergedRequestResult(serde_json::Error),
}

pub struct RoutedExecutor {
    request: ChatCompletionRequestWithTools<RoutingStrategy>,
}

impl RoutedExecutor {
    pub fn new(request: ChatCompletionRequestWithTools<RoutingStrategy>) -> Self {
        Self { request }
    }

    pub async fn execute(
        &self,
        executor_context: &ExecutorContext,
        memory_storage: Option<Arc<Mutex<InMemoryStorage>>>,
        project_id: Option<&uuid::Uuid>,
        thread_id: Option<&String>,
    ) -> Result<HttpResponse, GatewayApiError> {
        let span = Span::current();

        let mut targets = vec![(self.request.clone(), None)];

        let mut depth = 0;
        while let Some((mut request, target)) = targets.pop() {
            depth += 1;
            if depth > MAX_DEPTH {
                return Err(GatewayApiError::GatewayError(GatewayError::CustomError(
                    "Max depth reached".to_string(),
                )));
            }

            if let Some(t) = target {
                request.router = None;
                request = Self::merge_request_with_target(&request, &t)?;
            }

            if let Some(router) = &request.router {
                let router_name = request
                    .request
                    .model
                    .split('/')
                    .next_back()
                    .expect("Model name should not be empty")
                    .to_string();
                span.record("router_name", &router_name);

                let span = tracing::info_span!(
                    target: "vllora::user_tracing::request_routing",
                    SPAN_REQUEST_ROUTING,
                    router_name = router_name,
                    before = JsonValue(&serde_json::to_value(&request.request)?).as_value(),
                    router_resolution = field::Empty,
                    after = field::Empty
                );

                let llm_router = LlmRouter {
                    name: router.name.clone().unwrap_or("dynamic".to_string()),
                    strategy: router.strategy.clone(),
                    targets: router.targets.clone(),
                    metrics_duration: None,
                };

                let metrics = match &memory_storage {
                    Some(storage) => {
                        let guard = storage.lock().await;
                        guard.get_all_counters().await
                    }
                    None => BTreeMap::new(),
                };

                // Create metrics repository from the fetched metrics
                let metrics_repository = InMemoryMetricsRepository::new(metrics);

                let interceptor_factory = executor_context.get_interceptor_factory();
                let executor_result = llm_router
                    .route(
                        request.request.clone(),
                        request.extra.as_ref(),
                        Arc::clone(&executor_context.model_metadata_factory),
                        executor_context.metadata.clone(),
                        &metrics_repository,
                        interceptor_factory,
                    )
                    .instrument(span.clone())
                    .await;

                match executor_result {
                    Ok(routing_result) => {
                        for t in routing_result.targets.iter().rev() {
                            targets.push((request.clone(), Some(t.clone())));
                        }
                    }
                    Err(e) => {
                        tracing::error!("Router error: {}, route ignored", e);
                    }
                }
            } else {
                let result =
                    Self::execute_request(&request, executor_context, project_id, thread_id)
                        .instrument(span.clone())
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
        request: &ChatCompletionRequestWithTools<RoutingStrategy>,
        executor_context: &ExecutorContext,
        project_id: Option<&uuid::Uuid>,
        thread_id: Option<&String>,
    ) -> Result<HttpResponse, GatewayApiError> {
        let span = tracing::Span::current();
        span.record("request", &serde_json::to_string(&request)?);
        let trace_id = span.context().span().span_context().trace_id();

        let model_name = request.request.model.clone();

        let llm_model = match executor_context
            .model_metadata_factory
            .get_model_metadata(&request.request.model, false, false, project_id)
            .await
        {
            Ok(model) => model,
            Err(GatewayApiError::GatewayError(GatewayError::ModelError(_))) => {
                let model_name = request.request.model.clone();
                let model_parts = model_name.split('/').collect::<Vec<&str>>();
                let provider = model_parts.first().expect("Provider should not be empty");
                let model = model_parts.last().expect("Model should not be empty");
                //Proxying model call without details
                ModelMetadata {
                    model: model.to_string(),
                    inference_provider: InferenceProvider {
                        provider: InferenceModelProvider::from(provider.to_string()),
                        model_name: model.to_string(),
                        endpoint: None,
                    },
                    ..Default::default()
                }
            }
            Err(e) => {
                return Err(e);
            }
        };
        let response = execute(
            request,
            executor_context,
            span.clone(),
            StreamCacheContext::default(),
            BasicCacheContext::default(),
            &llm_model,
        )
        .instrument(span.clone())
        .await?;

        let mut response_builder = HttpResponse::Ok();
        let builder: &mut actix_web::HttpResponseBuilder = response_builder
            .insert_header(("X-Trace-Id", trace_id_uuid(trace_id).to_string()))
            .insert_header(("X-Model-Name", model_name.clone()))
            .insert_header((
                "X-Provider-Name",
                llm_model.inference_provider.provider.to_string(),
            ));

        if let Some(thread_id) = thread_id {
            builder.insert_header(("X-Thread-Id", thread_id.to_string()));
        }

        match response {
            Left(result_stream) => {
                let stream = result_stream?;

                // Pin the stream to heap
                let mut stream = Box::pin(stream);

                // Check first element for error
                let first = match stream.as_mut().next().await {
                    Some(Ok(delta)) => delta,
                    Some(Err(e)) => {
                        //todo: Fix
                        return Err(GatewayApiError::CustomError(e.to_string()));
                    }
                    None => {
                        return Err(GatewayApiError::GatewayError(GatewayError::CustomError(
                            "Empty response from model".to_string(),
                        )));
                    }
                };

                let price = llm_model.price.clone();
                let cost_calculator = executor_context.cost_calculator.clone();
                let model_name = llm_model.model.clone();
                let result = futures::stream::once(async { Ok(first) })
                    .chain(stream)
                    .then(move |delta| {
                        let price = price.clone();
                        let cost_calculator = cost_calculator.clone();
                        let model_name = model_name.clone();
                        async move {
                            let r = match delta {
                                Ok(delta) => {
                                    let mut delta: ChatCompletionChunk = delta.into();
                                    delta.model = model_name.clone();
                                    if let Some(usage) = delta.usage.as_mut() {
                                        let u = CompletionModelUsage {
                                            input_tokens: usage.prompt_tokens as u32,
                                            output_tokens: usage.completion_tokens as u32,
                                            total_tokens: usage.total_tokens as u32,
                                            prompt_tokens_details: usage
                                                .prompt_tokens_details
                                                .clone(),
                                            completion_tokens_details: usage
                                                .completion_tokens_details
                                                .clone(),
                                            ..Default::default()
                                        };
                                        usage.cost = cost_calculator
                                            .calculate_cost(
                                                &price,
                                                &Usage::CompletionModelUsage(u),
                                                &CredentialsIdent::Own,
                                            )
                                            .await?
                                            .cost;
                                    }
                                    let json_str = serde_json::to_string(&delta).unwrap();
                                    format!("data: {json_str}\n\n")
                                }
                                Err(e) => {
                                    let result = serde_json::to_string(&HashMap::from([(
                                        "error",
                                        e.to_string(),
                                    )]))
                                    .unwrap_or_else(|e| {
                                        format!("{{\"error\": \"Failed to serialize chunk: {e}\"}}")
                                    });

                                    format!("data: {result}\n\n")
                                }
                            };
                            Ok(Bytes::from(r))
                        }
                    })
                    .chain(futures::stream::once(async {
                        Ok::<_, GatewayApiError>(Bytes::from("data: [DONE]\n\n"))
                    }))
                    .instrument(span.clone());

                tracing::error!("end stream");
                Ok(builder.content_type("text/event-stream").streaming(result))
            }
            Right(completions_response) => Ok(builder.json(completions_response?)),
        }
    }

    fn merge_request_with_target(
        request: &ChatCompletionRequestWithTools<RoutingStrategy>,
        target: &HashMap<String, serde_json::Value>,
    ) -> Result<ChatCompletionRequestWithTools<RoutingStrategy>, RoutedExecutorError> {
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
