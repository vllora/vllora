use crate::model::ModelProviderInstance;
use crate::models::{InferenceProvider, ModelCapability, ModelIOFormats, ModelMetadata, ModelType};
use crate::types::credentials::{Credentials, VertexCredentials};
use crate::types::provider::{CompletionModelPrice, InferenceModelProvider, ModelPrice};
use crate::GatewayApiError;
use async_trait::async_trait;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone)]
enum GoogleClientKind {
    Vertex(Box<VertexCredentials>),
}
pub struct GoogleVertexModelProvider {
    http: reqwest::Client,
    client: GoogleClientKind,
}

#[derive(Serialize)]
pub struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    exp: usize,
    iat: usize,
}

#[derive(Deserialize, Debug)]
pub struct TokenResp {
    access_token: String,
}

impl GoogleVertexModelProvider {
    pub fn new(credentials: Credentials) -> Result<Self, GatewayApiError> {
        match credentials {
            Credentials::Vertex(vertex) => Ok(Self {
                http: reqwest::Client::new(),
                client: GoogleClientKind::Vertex(vertex),
            }),
            _ => Err(GatewayApiError::CustomError(
                "Unsupported credentials for Google Vertex".to_string(),
            )),
        }
    }

    async fn fetch_service_account_token(
        &self,
        creds: &VertexCredentials,
    ) -> Result<String, GatewayApiError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        let exp = now + 3600;
        let aud = creds
            .credentials
            .token_uri
            .clone()
            .unwrap_or("https://oauth2.googleapis.com/token".to_string());

        let claims = Claims {
            iss: &creds.credentials.client_email,
            scope: "https://www.googleapis.com/auth/cloud-platform",
            aud: &aud,
            exp,
            iat: now,
        };

        let header = Header::new(Algorithm::RS256);
        let pk_pem = creds.credentials.private_key.replace("\\n", "\n");
        let key = EncodingKey::from_rsa_pem(pk_pem.as_bytes())
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;
        let jwt = encode(&header, &claims, &key)
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

        let token_resp: TokenResp = self
            .http
            .post(aud)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", jwt.as_str()),
            ])
            .send()
            .await
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?
            .json()
            .await
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

        Ok(token_resp.access_token)
    }
}

#[async_trait]
impl ModelProviderInstance for GoogleVertexModelProvider {
    async fn get_private_models(&self) -> Result<Vec<ModelMetadata>, GatewayApiError> {
        match &self.client {
            GoogleClientKind::Vertex(creds) => {
                #[derive(serde::Deserialize)]
                struct VertexModelsResponse {
                    models: Option<Vec<VertexModelEntry>>,
                }

                #[derive(serde::Deserialize, Debug, Serialize)]
                struct VertexDeployedModel {
                    endpoint: String,
                    #[serde(rename = "deployedModelId")]
                    deployed_model_id: String,
                }

                #[derive(serde::Deserialize, Debug)]
                struct VertexModelEntry {
                    name: String,
                    #[serde(default)]
                    description: Option<String>,
                    #[allow(dead_code)]
                    #[serde(rename = "displayName")]
                    display_name: Option<String>,
                    #[serde(rename = "deployedModels")]
                    deployed_models: Option<Vec<VertexDeployedModel>>,
                }

                let token = self.fetch_service_account_token(creds).await?;
                let url = format!(
                    "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/models",
                    creds.region, creds.credentials.project_id, creds.region
                );

                let resp: VertexModelsResponse = self
                    .http
                    .get(&url)
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|e| GatewayApiError::CustomError(e.to_string()))?
                    .json()
                    .await
                    .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

                let mut out = Vec::new();
                for m in resp.models.unwrap_or_default() {
                    // Name format: projects/.../locations/.../publishers/google/models/<model_id>
                    let model_name = m
                        .name
                        .split('/')
                        .next_back()
                        .map(|s| s.to_string())
                        .unwrap_or(m.name.clone());
                    let input_formats = vec![ModelIOFormats::Text];
                    let output_formats = vec![ModelIOFormats::Text];
                    let capabilities = vec![ModelCapability::Tools];
                    let metadata = ModelMetadata {
                        model: model_name.clone(),
                        model_provider: "google".to_string(),
                        inference_provider: InferenceProvider {
                            // Execution path is not implemented for Vertex yet; we mark Gemini for compatibility
                            provider: InferenceModelProvider::VertexAI,
                            model_name: model_name.clone(),
                            endpoint: m
                                .deployed_models
                                .as_ref()
                                .map(|models| serde_json::to_string(models).unwrap()),
                        },
                        price: ModelPrice::Completion(CompletionModelPrice {
                            per_input_token: 0.0,
                            per_output_token: 0.0,
                            per_cached_input_token: None,
                            per_cached_input_write_token: None,
                            valid_from: None,
                        }),
                        input_formats,
                        output_formats,
                        capabilities,
                        r#type: ModelType::Completions,
                        limits: crate::models::Limits::new(0),
                        description: m.description.unwrap_or_default(),
                        parameters: None,
                        benchmark_info: None,
                        virtual_model_id: None,
                        min_service_level: 0,
                        release_date: None,
                        license: None,
                        knowledge_cutoff_date: None,
                    };
                    out.push(metadata);
                }
                Ok(out)
            }
        }
    }
}
