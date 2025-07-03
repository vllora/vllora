use std::collections::HashMap;
use std::sync::Arc;

use crate::handler::CallbackHandlerFn;
use crate::handler::ModelEventWithDetails;
use crate::llm_gateway::provider::Provider;
use crate::model::image_generation::initialize_image_generation;
use crate::model::types::ModelEvent;
use crate::models::ModelMetadata;
use crate::types::engine::ImageGenerationModelDefinition;
use crate::types::gateway::CreateImageRequest;
use crate::types::image::ImagesResponse;
use crate::types::provider::InferenceModelProvider;
use crate::GatewayError;
use crate::{
    model::types::ModelEventType,
    types::{
        credentials::Credentials,
        engine::{Model, ModelTools, ModelType},
        gateway::CostCalculator,
    },
};
use actix_web::HttpRequest;
use tracing::Span;
use tracing_futures::Instrument;

use super::get_key_credentials;
use super::ProvidersConfig;

pub async fn handle_image_generation(
    mut request: CreateImageRequest,
    callback_handler: &CallbackHandlerFn,
    llm_model: &ModelMetadata,
    key_credentials: Option<&Credentials>,
    cost_calculator: Arc<Box<dyn CostCalculator>>,
    tags: HashMap<String, String>,
    req: HttpRequest,
) -> Result<ImagesResponse, GatewayError> {
    let span = Span::current();
    request.model = llm_model.inference_provider.model_name.clone();

    let providers_config = req.app_data::<ProvidersConfig>().cloned();
    let key = get_key_credentials(
        key_credentials,
        providers_config.as_ref(),
        &llm_model.inference_provider.provider.to_string(),
    );
    let engine = Provider::get_image_engine_for_model(llm_model, &request, key.as_ref())?;

    let api_provider_name = match &llm_model.inference_provider.provider {
        InferenceModelProvider::Proxy(provider) => provider.clone(),
        _ => engine.provider_name().to_string(),
    };

    let db_model = Model {
        name: llm_model.model.clone(),
        description: None,
        provider_name: api_provider_name.clone(),
        prompt_name: None,
        model_params: HashMap::new(),
        tools: ModelTools(vec![]),
        model_type: ModelType::ImageGeneration,
        response_schema: None,
        credentials: key_credentials.cloned(),
    };

    let image_model_definition = ImageGenerationModelDefinition {
        name: llm_model.model.clone(),
        engine,
        db_model: db_model.clone(),
    };

    let cost_calculator = cost_calculator.clone();
    let callback_handler = callback_handler.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(1000);

    let handle = tokio::spawn(async move {
        let mut stop_event = None;
        while let Some(Some(msg)) = rx.recv().await {
            if let ModelEvent {
                event: ModelEventType::ImageGenerationFinish(e),
                ..
            } = &msg
            {
                stop_event = Some(e.clone());
            }

            callback_handler.on_message(ModelEventWithDetails::new(msg, Some(db_model.clone())));
        }

        stop_event
    });

    let model = initialize_image_generation(
        image_model_definition.clone(),
        Some(cost_calculator.clone()),
        llm_model.inference_provider.endpoint.as_deref(),
        Some(llm_model.model_provider.as_str()),
    )
    .await
    .map_err(|e| GatewayError::CustomError(e.to_string()))?;

    let result = model
        .create_new(&request, tx, tags.clone())
        .instrument(span.clone())
        .await?;

    let _stop_event = handle.await.unwrap();

    Ok(result)
}
