use crate::model::ModelProviderInstance;
use crate::models::{InferenceProvider, ModelCapability, ModelIOFormats, ModelMetadata, ModelType};
use crate::types::credentials::Credentials;
use crate::types::provider::{CompletionModelPrice, InferenceModelProvider, ModelPrice};
use crate::GatewayApiError;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct AzureDeployment {
    id: String,
    model: String,
    status: String,
    created_at: i64,
    updated_at: i64,
    object: String,
    scale_settings: ScaleSettings,
    owner: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScaleSettings {
    scale_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AzureDeploymentsResponse {
    data: Vec<AzureDeployment>,
    object: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AzureModelCapabilities {
    fine_tune: bool,
    inference: bool,
    completion: bool,
    chat_completion: bool,
    embeddings: bool,
    scale_types: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AzureModelDeprecation {
    inference: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AzureModel {
    capabilities: AzureModelCapabilities,
    lifecycle_status: String,
    deprecation: Option<AzureModelDeprecation>,
    id: String,
    status: String,
    created_at: i64,
    updated_at: i64,
    object: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AzureModelsResponse {
    data: Vec<AzureModel>,
    object: String,
}

pub struct AzureModelProvider {
    credentials: Credentials,
    client: Client,
}

impl AzureModelProvider {
    pub fn new(credentials: Credentials) -> Self {
        Self {
            credentials,
            client: Client::new(),
        }
    }

    fn get_api_key_and_endpoint(&self) -> Result<(String, String), GatewayApiError> {
        match &self.credentials {
            Credentials::ApiKeyWithEndpoint { api_key, endpoint } => {
                Ok((api_key.clone(), endpoint.clone()))
            }
            _ => Err(GatewayApiError::CustomError(
                "Azure OpenAI requires both API key and endpoint".to_string(),
            )),
        }
    }

    async fn fetch_deployments(&self) -> Result<Vec<AzureDeployment>, GatewayApiError> {
        let (api_key, endpoint) = self.get_api_key_and_endpoint()?;

        // Extract the base URL from the endpoint
        let base_url = if endpoint.contains("/openai/deployments") {
            endpoint
                .split("/openai/deployments")
                .next()
                .unwrap_or(&endpoint)
        } else {
            &endpoint
        };

        let deployments_url = format!(
            "{}/openai/deployments?api-version=2023-03-15-preview",
            base_url
        );

        let response = self
            .client
            .get(&deployments_url)
            .header("api-key", api_key)
            .send()
            .await
            .map_err(|e| {
                GatewayApiError::CustomError(format!("Failed to fetch Azure deployments: {}", e))
            })?;

        if !response.status().is_success() {
            return Err(GatewayApiError::CustomError(format!(
                "Azure API returned error status: {}",
                response.status()
            )));
        }

        let deployments_response: AzureDeploymentsResponse =
            response.json().await.map_err(|e| {
                GatewayApiError::CustomError(format!("Failed to parse Azure response: {}", e))
            })?;

        Ok(deployments_response.data)
    }

    async fn fetch_models(&self) -> Result<Vec<AzureModel>, GatewayApiError> {
        let (api_key, endpoint) = self.get_api_key_and_endpoint()?;

        // Extract the base URL from the endpoint
        let base_url = if endpoint.contains("/openai/deployments") {
            endpoint
                .split("/openai/deployments")
                .next()
                .unwrap_or(&endpoint)
        } else {
            &endpoint
        };

        let models_url = format!("{}/openai/models?api-version=2023-03-15-preview", base_url);

        let response = self
            .client
            .get(&models_url)
            .header("api-key", api_key)
            .send()
            .await
            .map_err(|e| {
                GatewayApiError::CustomError(format!("Failed to fetch Azure models: {}", e))
            })?;

        if !response.status().is_success() {
            return Err(GatewayApiError::CustomError(format!(
                "Azure API returned error status: {}",
                response.status()
            )));
        }

        let models_response: AzureModelsResponse = response.json().await.map_err(|e| {
            GatewayApiError::CustomError(format!("Failed to parse Azure models response: {}", e))
        })?;

        Ok(models_response.data)
    }

    fn get_model_capabilities(&self, model_info: Option<&AzureModel>) -> Vec<ModelCapability> {
        let mut capabilities = Vec::new();

        // Check if we have detailed model info
        if let Some(model_info) = model_info {
            // Add capabilities based on the model's capabilities
            if model_info.capabilities.chat_completion {
                capabilities.push(ModelCapability::Tools);
            }
        }

        capabilities
    }

    // At the moment, we don't have any limits for Azure OpenAI
    fn get_model_limits(&self, _model_name: &str) -> u32 {
        0
    }

    // At the moment, we don't have any pricing for Azure OpenAI
    fn get_model_pricing(&self, _model_name: &str) -> ModelPrice {
        ModelPrice::Completion(CompletionModelPrice {
            per_input_token: 0.0,
            per_output_token: 0.0,
            per_cached_input_token: None,
            per_cached_input_write_token: None,
            valid_from: None,
        })
    }

    fn get_model_type(&self, model_name: &str, model_info: Option<&AzureModel>) -> ModelType {
        // Check if we have detailed model info
        if let Some(model_info) = model_info {
            if model_info.capabilities.embeddings {
                return ModelType::Embeddings;
            }
            if model_info.capabilities.chat_completion || model_info.capabilities.completion {
                return ModelType::Completions;
            }
        }

        // Fallback to name-based detection
        if model_name.contains("text-embedding") || model_name.contains("embedding") {
            ModelType::Embeddings
        } else {
            ModelType::Completions
        }
    }

    fn get_input_output_formats(&self) -> (Vec<ModelIOFormats>, Vec<ModelIOFormats>) {
        // For now, most Azure OpenAI models support text input/output
        // In the future, this could be enhanced based on model capabilities
        let input_formats = vec![ModelIOFormats::Text];
        let output_formats = vec![ModelIOFormats::Text];

        (input_formats, output_formats)
    }
}

#[async_trait]
impl ModelProviderInstance for AzureModelProvider {
    async fn get_private_models(&self) -> Result<Vec<ModelMetadata>, GatewayApiError> {
        // Fetch both deployments and models
        let (deployments, models) =
            tokio::try_join!(self.fetch_deployments(), self.fetch_models())?;

        let mut model_info_map = HashMap::new();
        for model in models {
            model_info_map.insert(model.id.clone(), model.clone());
        }

        let mut models_metadata = Vec::new();

        for deployment in deployments {
            // Only include successful deployments
            if deployment.status != "succeeded" {
                continue;
            }

            let model_name = deployment.model.clone();
            let deployment_id = deployment.id.clone();

            // Get model info if available
            let model_info = model_info_map.get(&model_name);

            let capabilities = self.get_model_capabilities(model_info);
            let limits = self.get_model_limits(&model_name);
            let price = self.get_model_pricing(&model_name);
            let model_type = self.get_model_type(&model_name, model_info);
            let (input_formats, output_formats) = self.get_input_output_formats();

            if model_type != ModelType::Completions {
                continue;
            }
            let metadata = ModelMetadata {
                model: deployment_id.clone(),
                model_provider: "azure".to_string(),
                inference_provider: InferenceProvider {
                    provider: InferenceModelProvider::Proxy("azure".to_string()),
                    model_name: deployment_id,
                    endpoint: None, // The endpoint is already configured in the credentials
                },
                price,
                input_formats,
                output_formats,
                capabilities,
                r#type: model_type,
                limits: crate::models::Limits::new(limits),
                description: format!("Azure OpenAI deployment of {}", model_name),
                parameters: None,
                benchmark_info: None,
                virtual_model_id: None,
                min_service_level: 0,
                release_date: None,
                license: None,
                knowledge_cutoff_date: None,
            };

            models_metadata.push(metadata);
        }

        Ok(models_metadata)
    }
}
