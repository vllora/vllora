use std::collections::HashMap;

use crate::model::error::ModelError;
use crate::telemetry::events::SPAN_OPENAI;
use async_openai::config::Config;
use async_openai::{config::OpenAIConfig, Client};

use crate::model::types::ModelEventType;
use crate::{
    error::GatewayError,
    model::{
        openai::openai_client,
        types::{ImageGenerationFinishEvent, ModelEvent},
        CredentialsIdent,
    },
    types::{
        credentials::ApiKeyCredentials,
        gateway::{CreateImageRequest, ImageQuality, ImageResponseFormat, ImageSize, ImageStyle},
        image::ImagesResponse,
    },
    GatewayResult,
};

use super::ImageGenerationModelInstance;
use crate::model::JsonValue;
use secrecy::ExposeSecret;
use serde::Deserialize;
use tracing::field;
use valuable::Valuable;

#[derive(Debug, Deserialize, Clone)]
pub struct OpenAIReqwestError {
    pub error: ApiError,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiError {
    pub message: String,
    pub r#type: Option<String>,
    pub param: Option<String>,
    pub code: Option<String>,
}

#[derive(Clone)]
pub struct OpenAIImageGeneration {
    client: Client<OpenAIConfig>,
    credentials_ident: CredentialsIdent,
}

impl OpenAIImageGeneration {
    pub fn new(
        credentials: Option<&ApiKeyCredentials>,
        client: Option<Client<OpenAIConfig>>,
        endpoint: Option<&str>,
    ) -> Result<Self, ModelError> {
        Ok(OpenAIImageGeneration {
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Vllora),
            client: client.unwrap_or(openai_client(credentials, endpoint)?),
        })
    }

    fn generate_event(
        &self,
        model_name: &str,
        quality: Option<&ImageQuality>,
        size: Option<&ImageSize>,
        count_of_images: u8,
        steps: u8,
    ) -> ImageGenerationFinishEvent {
        ImageGenerationFinishEvent {
            model_name: model_name.to_string(),
            quality: quality
                .map(|q| q.to_string())
                .unwrap_or("standard".to_string()),
            size: size.cloned().unwrap_or(ImageSize::Size1024x1024),
            count_of_images,
            steps,
            credentials_ident: self.credentials_ident.clone(),
        }
    }

    fn map_size(
        &self,
        size: Option<&ImageSize>,
    ) -> Option<Result<async_openai::types::ImageSize, GatewayError>> {
        size.map(|s| match s {
            crate::types::gateway::ImageSize::Size256x256 => {
                Ok(async_openai::types::ImageSize::S256x256)
            }
            crate::types::gateway::ImageSize::Size512x512 => {
                Ok(async_openai::types::ImageSize::S512x512)
            }
            crate::types::gateway::ImageSize::Size1024x1024 => {
                Ok(async_openai::types::ImageSize::S1024x1024)
            }
            crate::types::gateway::ImageSize::Size1792x1024 => {
                Ok(async_openai::types::ImageSize::S1792x1024)
            }
            crate::types::gateway::ImageSize::Size1024x1792 => {
                Ok(async_openai::types::ImageSize::S1024x1792)
            }
            crate::types::gateway::ImageSize::Other((width, height)) => Err(
                GatewayError::CustomError(format!("Unsupported image size: {width}x{height}")),
            ),
        })
    }

    fn map_quality(
        &self,
        quality: Option<&ImageQuality>,
    ) -> Option<async_openai::types::ImageQuality> {
        quality.map(|q| match q {
            crate::types::gateway::ImageQuality::SD => async_openai::types::ImageQuality::Standard,
            crate::types::gateway::ImageQuality::HD => async_openai::types::ImageQuality::HD,
        })
    }
}

#[async_trait::async_trait]
impl ImageGenerationModelInstance for OpenAIImageGeneration {
    async fn create_new(
        &self,
        request: &CreateImageRequest,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ImagesResponse> {
        let input = serde_json::to_string(request)?;
        let call_span = tracing::info_span!(target: "vllora::user_tracing::models::openai::image_generation", SPAN_OPENAI, input = input, output = field::Empty, error = field::Empty, usage = field::Empty, ttft = field::Empty, tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value());

        let size = self.map_size(request.size.as_ref());

        let size = match size {
            Some(Ok(s)) => Some(s),
            Some(Err(e)) => return Err(e),
            None => None,
        };

        let quality = self.map_quality(request.quality.as_ref());

        let model = serde_json::from_str(&format!("\"{}\"", request.model))?;

        let r = async_openai::types::CreateImageRequest {
            prompt: request.prompt.clone(),
            n: request.n,
            size,
            response_format: request.response_format.as_ref().map(|f| match f {
                ImageResponseFormat::Url => async_openai::types::ImageResponseFormat::Url,
                ImageResponseFormat::B64Json => async_openai::types::ImageResponseFormat::B64Json,
            }),
            user: request.user.clone(),
            model: Some(model),
            quality,
            style: request.style.as_ref().map(|s| match s {
                ImageStyle::Vivid => async_openai::types::ImageStyle::Vivid,
                ImageStyle::Natural => async_openai::types::ImageStyle::Natural,
            }),
            moderation: request.moderation.as_ref().map(|m| match m {
                crate::types::gateway::ImageModeration::Auto => {
                    async_openai::types::ImageModeration::Auto
                }
                crate::types::gateway::ImageModeration::Low => {
                    async_openai::types::ImageModeration::Low
                }
            }),
        };

        let api_base = self.client.config().api_base().to_string();
        let api_key: String = self.client.config().api_key().expose_secret().to_string();

        let reqwest_client = reqwest::Client::new();
        let reqwest_result = reqwest_client
            .post(format!("{api_base}/images/generations"))
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&r)
            .send()
            .await?;

        if reqwest_result.status().is_success() {
            let result = reqwest_result.json::<ImagesResponse>().await?;

            let event = self.generate_event(
                &request.model,
                request.quality.as_ref(),
                request.size.as_ref(),
                request.n.unwrap_or(1),
                1,
            );

            tx.send(Some(ModelEvent::new(
                &call_span,
                ModelEventType::ImageGenerationFinish(event),
            )))
            .await
            .unwrap();

            Ok(result)
        } else {
            let r: OpenAIReqwestError = reqwest_result.json().await.map_err(|e| {
                call_span.record("error", e.to_string());
                GatewayError::CustomError(format!("Failed to generate image: {e}"))
            })?;
            Err(GatewayError::CustomError(format!(
                "Failed to generate image: {}",
                r.error.message
            )))
        }
    }
}
