use crate::model::async_trait;
use crate::model::error::ModelError;
use crate::model::openai_spec_client::openai_spec_client;
use crate::model::types::ModelEvent;
use crate::types::credentials::ApiKeyCredentials;
use crate::types::gateway::CreateImageRequest;
use crate::types::image::ImagesResponse;
use crate::GatewayResult;
use async_openai::config::OpenAIConfig;
use async_openai::Client;
use std::collections::HashMap;

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
