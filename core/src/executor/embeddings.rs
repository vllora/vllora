use std::collections::HashMap;
use std::sync::Arc;

use crate::handler::CallbackHandlerFn;
use crate::handler::ModelEventWithDetails;
use crate::llm_gateway::provider::Provider;
use crate::model::embeddings::initialize_embeddings_model_instance;
use crate::types::embed::EmbeddingResult;
use crate::GatewayError;
use actix_web::HttpRequest;
use tracing::Span;
use tracing_futures::Instrument;
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::credentials_ident::CredentialsIdent;
use vllora_llm::types::engine::EmbeddingsModelDefinition;
use vllora_llm::types::engine::Model;
use vllora_llm::types::gateway::CostCalculator;
use vllora_llm::types::gateway::CreateEmbeddingRequest;
use vllora_llm::types::models::ModelMetadata;
use vllora_llm::types::models::ModelType;
use vllora_llm::types::provider::InferenceModelProvider;
use vllora_llm::types::ModelEvent;
use vllora_llm::types::ModelEventType;

use super::get_key_credentials;
use super::ProvidersConfig;

pub async fn handle_embeddings(
    mut request: CreateEmbeddingRequest,
    callback_handler: &CallbackHandlerFn,
    llm_model: &ModelMetadata,
    key_credentials: Option<&Credentials>,
    cost_calculator: Arc<Box<dyn CostCalculator>>,
    tags: HashMap<String, String>,
    req: HttpRequest,
) -> Result<EmbeddingResult, GatewayError> {
    let span = Span::current();
    request.model = llm_model.inference_provider.model_name.clone();

    let providers_config = req.app_data::<ProvidersConfig>().cloned();
    let key = get_key_credentials(
        key_credentials,
        providers_config.as_ref(),
        &llm_model.inference_provider.provider.to_string(),
    );
    let engine = Provider::get_embeddings_engine_for_model(llm_model, &request, key.as_ref())?;

    let api_provider_name = match &llm_model.inference_provider.provider {
        InferenceModelProvider::Proxy(provider) => provider.clone(),
        _ => engine.provider_name().to_string(),
    };

    let db_model = Model {
        name: llm_model.model.clone(),
        inference_model_name: llm_model.inference_provider.model_name.clone(),
        provider_name: api_provider_name.clone(),
        model_type: ModelType::Embeddings,
        price: llm_model.price.clone(),
        credentials_ident: match key_credentials {
            Some(_) => CredentialsIdent::Own,
            _ => CredentialsIdent::Vllora,
        },
    };

    let embeddings_model_definition = EmbeddingsModelDefinition {
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
                event: ModelEventType::LlmStop(e),
                ..
            } = &msg
            {
                stop_event = Some(e.clone());
            }

            callback_handler.on_message(ModelEventWithDetails::new(msg, Some(db_model.clone())));
        }

        stop_event
    });

    let model = initialize_embeddings_model_instance(
        embeddings_model_definition.clone(),
        Some(cost_calculator.clone()),
        llm_model.inference_provider.endpoint.as_deref(),
        Some(llm_model.model_provider.as_str()),
    )
    .await
    .map_err(|e| GatewayError::CustomError(e.to_string()))?;

    let result = model
        .embed(&request, tx, tags.clone())
        .instrument(span.clone())
        .await?;

    let _stop_event = handle.await.unwrap();

    Ok(result)
}
