use crate::error::GatewayError;
use crate::executor::get_key_credentials;
use crate::executor::ProvidersConfig;
use crate::model::types::ModelEvent;
use crate::models::ModelMetadata;
use crate::responses::OpenAIResponses;
use crate::responses::Responses;
use crate::types::credentials::ApiKeyCredentials;
use crate::types::credentials::Credentials;
use actix_web::HttpRequest;
pub use async_openai::types::responses as ResponsesTypes;
use async_openai::types::responses::CreateResponse;
use async_openai::types::responses::Response;
pub use async_openai::Client;

pub async fn handle_create_response(
    request: &CreateResponse,
    key_credentials: Option<&Credentials>,
    llm_model: &ModelMetadata,
    req: &HttpRequest,
) -> Result<Response, GatewayError> {
    let mut custom_endpoint = None;
    let providers_config = req.app_data::<ProvidersConfig>().cloned();
    let key = match get_key_credentials(
        key_credentials,
        providers_config.as_ref(),
        &llm_model.inference_provider.provider.to_string(),
    ) {
        Some(Credentials::ApiKey(key)) => Some(key),
        Some(Credentials::ApiKeyWithEndpoint {
            api_key: key,
            endpoint,
        }) => {
            custom_endpoint = Some(endpoint);
            Some(ApiKeyCredentials { api_key: key })
        }
        _ => None,
    };

    let client = OpenAIResponses::new(key.as_ref(), custom_endpoint.as_deref()).unwrap();

    let (tx, _rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(1000);
    let response = client
        .invoke(request.clone(), Some(tx.clone()))
        .await
        .unwrap();

    Ok(response)
}
