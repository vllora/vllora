use crate::types::image::ImagesResponse;
use crate::GatewayResult;
use async_trait::async_trait;
use std::collections::HashMap;
use vllora_llm::async_openai::config::OpenAIConfig;
use vllora_llm::async_openai::Client;
use vllora_llm::client::error::ModelError;
use vllora_llm::provider::openai_spec_client::openai_spec_client;
use vllora_llm::types::credentials::ApiKeyCredentials;
use vllora_llm::types::gateway::CreateImageRequest;
use vllora_llm::types::ModelEvent;

use super::openai::OpenAIImageGeneration;
use super::ImageGenerationModelInstance;

#[derive(Clone)]
pub struct OpenAISpecModel {
    openai_model: OpenAIImageGeneration,
}
impl OpenAISpecModel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        credentials: Option<&ApiKeyCredentials>,
        endpoint: Option<&str>,
        provider_name: &str,
    ) -> Result<Self, ModelError> {
        let client: Client<OpenAIConfig> =
            openai_spec_client(credentials, endpoint, provider_name)?;
        let openai_model = OpenAIImageGeneration::new(credentials, Some(client), None)?;

        Ok(Self { openai_model })
    }
}

#[async_trait]
impl ImageGenerationModelInstance for OpenAISpecModel {
    async fn create_new(
        &self,
        request: &CreateImageRequest,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ImagesResponse> {
        self.openai_model.create_new(request, tx, tags).await
    }
}
