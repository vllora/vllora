use crate::credentials::GatewayCredentials;
use crate::error::GatewayError;
use crate::executor::context::ExecutorContext;
use crate::handler::ModelEventWithDetails;
use crate::model::responses::init_traced_responses_model_instance;
use crate::GatewayApiError;
pub use async_openai::types::responses as ResponsesTypes;
use async_openai::types::responses::CreateResponse;
use async_openai::types::responses::Response;
use vllora_llm::client::responses::Responses;
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::credentials_ident::CredentialsIdent;
use vllora_llm::types::engine::Model;
use vllora_llm::types::engine::ResponsesEngineParamsBuilder;
use vllora_llm::types::engine::ResponsesModelDefinition;
use vllora_llm::types::engine::ResponsesModelParams;
use vllora_llm::types::models::InferenceProvider;
use vllora_llm::types::models::ModelMetadata;
use vllora_llm::types::models::ModelType;
use vllora_llm::types::provider::InferenceModelProvider;
use vllora_llm::types::tools::ModelTools;
use vllora_llm::types::ModelEvent;

pub struct ResolvedResponsesModelContext {
    pub completion_model_definition: ResponsesModelDefinition,
    pub model_instance: Box<dyn Responses>,
    pub db_model: Model,
    pub llm_model: ModelMetadata,
}

pub async fn handle_create_response(
    request: &CreateResponse,
    executor_context: &ExecutorContext,
) -> Result<Response, GatewayApiError> {
    let responses_request = request.clone();

    let model_name = request.model.clone();
    let llm_model = match executor_context
        .model_metadata_factory
        .get_model_metadata(
            &model_name,
            false,
            false,
            Some(&executor_context.project_id),
        )
        .await
    {
        Ok(model) => model,
        Err(GatewayApiError::GatewayError(GatewayError::ModelError(_))) => {
            let model_name = request.model.clone();
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

    let key = GatewayCredentials::extract_key_from_model(
        &llm_model,
        &executor_context.project_id.to_string(),
        "default",
        executor_context.key_storage.as_ref().as_ref(),
    )
    .await
    .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    let resolved_model_context = resolve_model_instance(
        ModelTools::default(),
        &llm_model,
        key.as_ref(),
        executor_context,
    )
    .await?;

    let callback_handler = executor_context.callbackhandler.clone();
    let db_model = resolved_model_context.db_model.clone();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(1000);
    tokio::spawn(async move {
        while let Some(Some(msg)) = rx.recv().await {
            callback_handler.on_message(ModelEventWithDetails::new(msg, Some(db_model.clone())));
        }
    });
    // let is_stream = request.stream.unwrap_or(false);
    // if is_stream {
    //     let stream = resolved_model_context.model_instance.stream(responses_request, Some(tx.clone())).await?;
    //     Ok(stream)
    // } else {
    let response = resolved_model_context
        .model_instance
        .invoke(responses_request, Some(tx.clone()))
        .await?;
    Ok(response)
    // }
}

async fn resolve_model_instance(
    tools: ModelTools,
    llm_model: &ModelMetadata,
    key: Option<&Credentials>,
    executor_context: &ExecutorContext,
) -> Result<ResolvedResponsesModelContext, GatewayApiError> {
    let mut builder =
        ResponsesEngineParamsBuilder::new().with_provider(llm_model.inference_provider.clone());

    builder = builder.with_model_name(llm_model.inference_provider.model_name.clone());

    if let Some(credentials) = key {
        builder = builder.with_credentials(credentials.clone());
    }

    let engine = builder.build()?;

    let credentials_ident = if llm_model.inference_provider.provider
        == InferenceModelProvider::Proxy("vllora".to_string())
    {
        CredentialsIdent::Vllora
    } else {
        CredentialsIdent::Own
    };

    let db_model = Model {
        name: llm_model.model.clone(),
        inference_model_name: llm_model.inference_provider.model_name.clone(),
        provider_name: llm_model.inference_provider.provider.to_string(),
        model_type: ModelType::Responses,
        price: llm_model.price.clone(),
        credentials_ident,
    };

    let completion_model_definition = ResponsesModelDefinition {
        name: format!(
            "{}/{}",
            llm_model.inference_provider.provider, llm_model.model
        ),
        model_params: ResponsesModelParams {
            engine: engine.clone(),
            provider_name: llm_model.model_provider.to_string(),
        },
        tools,
        db_model: db_model.clone(),
    };

    let model_instance = init_traced_responses_model_instance(
        completion_model_definition.clone(),
        executor_context.clone(),
    )
    .await
    .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    Ok(ResolvedResponsesModelContext {
        completion_model_definition,
        model_instance,
        db_model,
        llm_model: llm_model.clone(),
    })
}
