use std::collections::HashMap;

use crate::executor::context::ExecutorContext;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc::channel;
use tracing_futures::Instrument;
use vllora_llm::async_openai::types::responses::{CreateResponse, Response};
use vllora_llm::{
    client::{
        error::ModelError,
        responses::{stream::ResponsesResultStream, Responses},
    },
    error::LLMResult,
    types::{
        credentials_ident::CredentialsIdent,
        engine::{ResponsesEngineParams, ResponsesModelDefinition, ResponsesModelParams},
        events::CustomEventType,
        gateway::{GatewayModelUsage, Usage},
        instance::init_responses_model_instance,
        models::ModelType,
        tools::ModelTools,
        CustomEvent, ModelEvent, ModelEventType,
    },
};
use vllora_telemetry::create_model_invoke_span;

pub struct TracedResponsesModel {
    definition: ResponsesModelDefinition,
    executor_context: ExecutorContext,
}

#[allow(clippy::too_many_arguments)]
pub async fn init_traced_responses_model_instance(
    definition: ResponsesModelDefinition,
    executor_context: ExecutorContext,
) -> Result<Box<dyn Responses>, ModelError> {
    Ok(Box::new(TracedResponsesModel {
        definition,
        executor_context,
    }))
}

#[derive(Clone, Serialize)]
struct TraceResponsesModelDefinition {
    pub name: String,
    pub provider_name: String,
    pub model_params: ResponsesModelParams,
    pub tools: ModelTools,
    pub model_type: ModelType,
}

impl TraceResponsesModelDefinition {
    pub fn sanitize_json(&self) -> LLMResult<Value> {
        let mut model = self.clone();

        match &mut model.model_params.engine {
            ResponsesEngineParams::OpenAi {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }
        }
        let model = serde_json::to_value(&model)?;
        Ok(model)
    }
}
impl From<ResponsesModelDefinition> for TraceResponsesModelDefinition {
    fn from(value: ResponsesModelDefinition) -> Self {
        Self {
            name: value.name,
            provider_name: value.model_params.provider_name.clone(),
            model_params: value.model_params,
            tools: value.tools,
            model_type: ModelType::Responses,
        }
    }
}

impl TracedResponsesModel {
    fn clean_input_trace(&self, input_vars: &HashMap<String, Value>) -> LLMResult<String> {
        let input_vars = input_vars.clone();
        let str = serde_json::to_string(&json!(input_vars))?;
        Ok(str)
    }
}

#[async_trait::async_trait]
impl Responses for TracedResponsesModel {
    async fn invoke(
        &self,
        mut request: CreateResponse,
        outer_tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> LLMResult<Response> {
        let credentials_ident = credentials_identifier_responses(&self.definition.model_params);
        let traced_model: TraceResponsesModelDefinition = self.definition.clone().into();
        let model = traced_model.sanitize_json()?;
        let model_str = serde_json::to_string(&model)?;

        let input = HashMap::new();
        let input_str = self.clean_input_trace(&input)?;
        let model_name = self.definition.name.clone();
        let provider_name = self.definition.db_model.provider_name.clone();

        let span = create_model_invoke_span!(
            &input_str,
            model_str,
            provider_name,
            model_name.clone(),
            self.definition.db_model.inference_model_name.to_string(),
            credentials_ident.to_string()
        );

        let instance = init_responses_model_instance(
            self.definition.model_params.engine.clone(),
            HashMap::new(),
        )
        .await?;

        request.model = Some(self.definition.db_model.inference_model_name.clone());

        let capacity = outer_tx.as_ref().map_or(10000, |tx| tx.max_capacity());
        if let Some(outer_tx) = outer_tx.as_ref() {
            outer_tx
                .send(Some(ModelEvent::new(
                    &span,
                    ModelEventType::Custom(CustomEvent::new(CustomEventType::SpanStart {
                        operation_name: "model_call".to_string(),
                        attributes: serde_json::json!({}),
                    })),
                )))
                .await?;
        }

        let cost_calculator = self.executor_context.cost_calculator.clone();
        let price = self.definition.db_model.price.clone();
        let _model_name_clone = model_name.clone();
        let _provider_name_clone = provider_name.clone();

        let (tx, mut rx) = channel::<Option<ModelEvent>>(capacity);
        tokio::spawn(async move {
            let mut usage = GatewayModelUsage::default();
            let mut total_cost = 0.0;
            while let Some(Some(msg)) = rx.recv().await {
                if let ModelEventType::LlmStop(llmfinish_event) = &msg.event {
                    let current_span = tracing::Span::current();
                    if let Some(output) = &llmfinish_event.output {
                        current_span.record("output", serde_json::to_string(output).unwrap());
                    }
                    if let Some(u) = &llmfinish_event.usage {
                        match cost_calculator
                            .calculate_cost(
                                &price,
                                &Usage::CompletionModelUsage(u.clone()),
                                &credentials_ident,
                            )
                            .await
                        {
                            Ok(mut c) => {
                                total_cost += c.cost;
                                c.cost = total_cost;
                                current_span.record("cost", serde_json::to_string(&c).unwrap());
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Error calculating cost: {:?} {:#?}",
                                    e,
                                    llmfinish_event
                                );
                            }
                        };

                        usage.add_usage(u);
                        current_span.record("usage", serde_json::to_string(u).unwrap());
                    }
                }
                if let Some(tx) = outer_tx.as_ref() {
                    let _ = tx.send(Some(msg)).await;
                }
            }
        });

        let result = instance.invoke(request, Some(tx)).await;

        result
    }

    async fn stream(
        &self,
        mut request: CreateResponse,
        outer_tx: Option<tokio::sync::mpsc::Sender<Option<ModelEvent>>>,
    ) -> LLMResult<ResponsesResultStream> {
        let credentials_ident = credentials_identifier_responses(&self.definition.model_params);
        let traced_model: TraceResponsesModelDefinition = self.definition.clone().into();
        let model = traced_model.sanitize_json()?;
        let model_str = serde_json::to_string(&model)?;

        let input = HashMap::new();
        let input_str = self.clean_input_trace(&input)?;
        let model_name = self.definition.name.clone();
        let provider_name = self.definition.db_model.provider_name.clone();

        let span = create_model_invoke_span!(
            &input_str,
            model_str,
            provider_name,
            model_name.clone(),
            self.definition.db_model.inference_model_name.to_string(),
            credentials_ident.to_string()
        );

        let instance = init_responses_model_instance(
            self.definition.model_params.engine.clone(),
            HashMap::new(),
        )
        .await?;

        request.model = Some(self.definition.db_model.inference_model_name.clone());

        let capacity = outer_tx.as_ref().map_or(10000, |tx| tx.max_capacity());
        let (tx, mut rx) = channel::<Option<ModelEvent>>(capacity);

        let cost_calculator = self.executor_context.cost_calculator.clone();
        let price = self.definition.db_model.price.clone();
        let _model_name_clone = model_name.clone();
        let _provider_name_clone = provider_name.clone();

        tokio::spawn(async move {
            let mut usage = GatewayModelUsage::default();
            let mut total_cost = 0.0;
            while let Some(Some(msg)) = rx.recv().await {
                if let ModelEventType::LlmStop(llmfinish_event) = &msg.event {
                    let current_span = tracing::Span::current();
                    if let Some(output) = &llmfinish_event.output {
                        current_span.record("output", serde_json::to_string(output).unwrap());
                    }
                    if let Some(u) = &llmfinish_event.usage {
                        match cost_calculator
                            .calculate_cost(
                                &price,
                                &Usage::CompletionModelUsage(u.clone()),
                                &credentials_ident,
                            )
                            .await
                        {
                            Ok(mut c) => {
                                total_cost += c.cost;
                                c.cost = total_cost;
                                current_span.record("cost", serde_json::to_string(&c).unwrap());
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Error calculating cost: {:?} {:#?}",
                                    e,
                                    llmfinish_event
                                );
                            }
                        };

                        usage.add_usage(u);
                        current_span.record("usage", serde_json::to_string(u).unwrap());
                    }
                }
                if let Some(tx) = outer_tx.as_ref() {
                    let _ = tx.send(Some(msg)).await;
                }
            }
        });

        let result = instance
            .stream(request, Some(tx))
            .instrument(span.clone())
            .await;

        result
    }
}

pub fn credentials_identifier_responses(model_params: &ResponsesModelParams) -> CredentialsIdent {
    let vllora_creds = match &model_params.engine {
        ResponsesEngineParams::OpenAi { credentials, .. } => credentials.is_none(),
    };

    if vllora_creds {
        CredentialsIdent::Vllora
    } else {
        CredentialsIdent::Own
    }
}
