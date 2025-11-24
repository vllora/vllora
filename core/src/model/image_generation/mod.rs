use std::collections::HashMap;
use std::sync::Arc;

use openai::OpenAIImageGeneration;
use serde::Serialize;
use serde_json::Value;
use tracing::info_span;
use tracing_futures::Instrument;
use valuable::Valuable;
use vllora_llm::client::error::ModelError;
use vllora_open::OpenAISpecModel;

use crate::types::image::ImagesResponse;
use crate::GatewayResult;
use vllora_llm::types::engine::{ImageGenerationEngineParams, ImageGenerationModelDefinition};
use vllora_llm::types::gateway::{
    CostCalculator, CreateImageRequest, ImageGenerationModelUsage, Usage,
};
use vllora_llm::types::ModelEventType;
use vllora_telemetry::events::{JsonValue, RecordResult, SPAN_MODEL_CALL};

use tokio::sync::mpsc::channel;
use vllora_llm::types::credentials_ident::CredentialsIdent;
use vllora_llm::types::ModelEvent;

pub mod openai;
pub mod vllora_open;

#[async_trait::async_trait]
pub trait ImageGenerationModelInstance: Sync + Send {
    async fn create_new(
        &self,
        request: &CreateImageRequest,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ImagesResponse>;
}

fn initialize_image_generation_model_instance(
    definition: ImageGenerationModelDefinition,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
    endpoint: Option<&str>,
    provider_name: Option<&str>,
) -> Result<Box<dyn ImageGenerationModelInstance>, ModelError> {
    match &definition.engine {
        ImageGenerationEngineParams::OpenAi {
            credentials,
            endpoint,
            ..
        } => Ok(Box::new(TracedImageGenerationModel {
            inner: OpenAIImageGeneration::new(
                credentials.clone().as_ref(),
                None,
                endpoint.as_ref().map(|s| s.as_str()),
            )?,
            definition: definition.clone(),
            cost_calculator: cost_calculator.clone(),
        })),
        ImageGenerationEngineParams::VlloraOpen { credentials, .. } => {
            Ok(Box::new(TracedImageGenerationModel {
                inner: OpenAISpecModel::new(
                    credentials.clone().as_ref(),
                    endpoint,
                    provider_name.unwrap(),
                )?,
                definition: definition.clone(),
                cost_calculator: cost_calculator.clone(),
            }))
        }
    }
}

pub async fn initialize_image_generation(
    definition: ImageGenerationModelDefinition,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
    endpoint: Option<&str>,
    provider_name: Option<&str>,
) -> Result<Box<dyn ImageGenerationModelInstance>, ModelError> {
    initialize_image_generation_model_instance(definition, cost_calculator, endpoint, provider_name)
}
pub struct TracedImageGenerationModel<Inner: ImageGenerationModelInstance> {
    inner: Inner,
    definition: ImageGenerationModelDefinition,
    cost_calculator: Option<Arc<Box<dyn CostCalculator>>>,
}

#[derive(Clone, Serialize)]
struct TracedImageGenerationModelDefinition {
    pub name: String,
    pub provider_name: String,
    pub engine_name: String,
    pub prompt_name: Option<String>,
    pub model_params: ImageGenerationModelDefinition,
    pub model_name: String,
}

impl TracedImageGenerationModelDefinition {
    pub fn sanitize_json(&self) -> GatewayResult<Value> {
        let mut model = self.clone();

        match &mut model.model_params.engine {
            ImageGenerationEngineParams::OpenAi {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }
            ImageGenerationEngineParams::VlloraOpen {
                ref mut credentials,
                ..
            } => {
                credentials.take();
            }
        }
        let model = serde_json::to_value(&model)?;
        Ok(model)
    }

    pub fn get_credentials_owner(&self) -> CredentialsIdent {
        match &self.model_params.engine {
            ImageGenerationEngineParams::OpenAi { credentials, .. } => match &credentials {
                Some(_) => CredentialsIdent::Own,
                None => CredentialsIdent::Vllora,
            },
            ImageGenerationEngineParams::VlloraOpen { credentials, .. } => match &credentials {
                Some(_) => CredentialsIdent::Own,
                None => CredentialsIdent::Vllora,
            },
        }
    }
}

impl From<ImageGenerationModelDefinition> for TracedImageGenerationModelDefinition {
    fn from(value: ImageGenerationModelDefinition) -> Self {
        Self {
            model_name: value.db_model.inference_model_name.clone(),
            name: value.name.clone(),
            provider_name: value.db_model.provider_name.clone(),
            engine_name: value.engine.engine_name().to_string(),
            prompt_name: None,
            model_params: value.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<Inner: ImageGenerationModelInstance> ImageGenerationModelInstance
    for TracedImageGenerationModel<Inner>
{
    async fn create_new(
        &self,
        request: &CreateImageRequest,
        outer_tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ImagesResponse> {
        let traced_model: TracedImageGenerationModelDefinition = self.definition.clone().into();
        let credentials_ident = traced_model.get_credentials_owner();
        let model = traced_model.sanitize_json()?;
        let model_str = serde_json::to_string(&model)?;
        let provider_name = self.definition.db_model.provider_name.clone();
        let request_str = serde_json::to_string(request)?;

        let (tx, mut rx) = channel::<Option<ModelEvent>>(outer_tx.max_capacity());
        let span = info_span!(
            target: "vllora::user_tracing::models", SPAN_MODEL_CALL,
            input = &request_str,
            model = model_str,
            provider_name = provider_name,
            output = tracing::field::Empty,
            error = tracing::field::Empty,
            credentials_identifier = credentials_ident.to_string(),
            cost = tracing::field::Empty,
            usage = tracing::field::Empty,
            tags = JsonValue(&serde_json::to_value(tags.clone())?).as_value(),
        );

        let cost_calculator = self.cost_calculator.clone();
        let price = self.definition.db_model.price.clone();
        tokio::spawn(
            async move {
                while let Some(Some(msg)) = rx.recv().await {
                    if let Some(cost_calculator) = cost_calculator.as_ref() {
                        if let ModelEventType::ImageGenerationFinish(generation_finish_event) =
                            &msg.event
                        {
                            let s = tracing::Span::current();
                            let u = ImageGenerationModelUsage {
                                quality: generation_finish_event.quality.clone(),
                                size: generation_finish_event.size.clone().into(),
                                images_count: generation_finish_event.count_of_images,
                                steps_count: generation_finish_event.steps,
                            };
                            match cost_calculator
                                .calculate_cost(
                                    &price,
                                    &Usage::ImageGenerationModelUsage(u.clone()),
                                    &credentials_ident,
                                )
                                .await
                            {
                                Ok(c) => {
                                    s.record("cost", serde_json::to_string(&c).unwrap());
                                }
                                Err(e) => {
                                    tracing::error!("Error calculating cost: {:?}", e);
                                }
                            };

                            s.record("usage", serde_json::to_string(&u).unwrap());
                        }
                    }

                    outer_tx.send(Some(msg)).await.unwrap();
                }
            }
            .instrument(span.clone()),
        );

        async {
            let result = self.inner.create_new(request, tx, tags).await;
            let _ = result.as_ref().map(|r| r.data.len()).record();

            result
        }
        .instrument(span)
        .await
    }
}
