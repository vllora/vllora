use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Get cloud API URL
fn get_api_url() -> String {
    std::env::var("LANGDB_API_URL").unwrap_or_else(|_| "https://api.langdb.cloud".to_string())
}

/// Client for interacting with LangDB Cloud finetune API
pub struct LangdbCloudFinetuneClient {
    client: reqwest::Client,
    api_url: String,
}

impl LangdbCloudFinetuneClient {
    /// Create a new client with API key
    pub fn new(api_key: String) -> Result<Self, String> {
        let mut headers = HeaderMap::new();
        let authorization_value = format!("Bearer {}", api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&authorization_value)
                .map_err(|e| format!("Failed to create authorization header: {}", e))?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self {
            client,
            api_url: get_api_url(),
        })
    }

    /// Upload a dataset file (JSONL format)
    pub async fn upload_dataset(
        &self,
        jsonl_data: Vec<u8>,
    ) -> Result<UploadDatasetResponse, String> {
        let mut form = reqwest::multipart::Form::new();

        let jsonl_part = reqwest::multipart::Part::bytes(jsonl_data)
            .file_name("training.jsonl")
            .mime_str("application/x-ndjson")
            .map_err(|e| format!("Failed to create multipart part: {}", e))?;
        form = form.part("file", jsonl_part);

        let url = format!("{}/finetune/datasets", self.api_url);

        let request = self.client.post(&url).multipart(form);

        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        let body: UploadDatasetResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(body)
    }

    /// Create a reinforcement fine-tuning job
    pub async fn create_reinforcement_job(
        &self,
        request: CreateReinforcementFinetuningJobRequest,
    ) -> Result<FinetuningJobResponse, String> {
        let url = format!("{}/finetune/reinforcement-jobs", self.api_url);

        let req = self.client.post(&url).json(&request);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        let body: FinetuningJobResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(body)
    }

    /// Get reinforcement fine-tuning job status
    pub async fn get_reinforcement_job_status(
        &self,
        job_id: &str,
    ) -> Result<ReinforcementJobStatusResponse, String> {
        let url = format!(
            "{}/finetune/reinforcement-jobs/{}/status",
            self.api_url, job_id
        );

        let req = self.client.get(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        let body: ReinforcementJobStatusResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(body)
    }

    /// List reinforcement fine-tuning jobs
    pub async fn list_reinforcement_jobs(
        &self,
        limit: Option<u32>,
        after: Option<String>,
    ) -> Result<Vec<FinetuningJobResponse>, String> {
        let mut url = format!("{}/finetune/reinforcement-jobs", self.api_url);
        let mut query_params = Vec::new();
        if let Some(limit) = limit {
            query_params.push(format!("limit={}", limit));
        }
        if let Some(after) = &after {
            query_params.push(format!("after={}", after));
        }
        if !query_params.is_empty() {
            url.push('?');
            url.push_str(&query_params.join("&"));
        }

        let req = self.client.get(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        let body: Vec<FinetuningJobResponse> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(body)
    }

    /// Deploy a fine-tuned model
    pub async fn deploy_model(
        &self,
        request: CreateDeploymentRequest,
    ) -> Result<DeploymentResponse, String> {
        let url = format!("{}/finetune/deployments", self.api_url);

        let req = self.client.post(&url).json(&request);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        let body: DeploymentResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(body)
    }

    /// Delete a deployment
    pub async fn delete_deployment(&self, deployment_id: &str) -> Result<(), String> {
        let url = format!("{}/finetune/deployments/{}", self.api_url, deployment_id);

        let req = self.client.delete(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        Ok(())
    }
}

// Response types
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadDatasetResponse {
    pub dataset_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReinforcementFinetuningJobRequest {
    pub dataset: String,
    pub base_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation_dataset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_config: Option<ReinforcementTrainingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_parameters: Option<ReinforcementInferenceParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReinforcementTrainingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learning_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora_rank: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epochs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_accumulation_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learning_rate_warmup_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size_samples: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReinforcementInferenceParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_candidates_count: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinetuningJobResponse {
    pub id: uuid::Uuid,
    pub provider_job_id: String,
    pub status: String,
    pub base_model: String,
    pub fine_tuned_model: Option<String>,
    pub provider: String,
    pub hyperparameters: Option<serde_json::Value>,
    pub suffix: Option<String>,
    pub error_message: Option<String>,
    pub training_file_id: String,
    pub validation_file_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReinforcementJobStatusResponse {
    pub provider_job_id: String,
    pub status: String,
    pub fine_tuned_model: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDeploymentRequest {
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_replica_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_replica_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autoscaling_policy: Option<AutoscalingPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoscalingPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_up_window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_down_window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_to_zero_window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_targets: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeploymentResponse {
    pub deployment_id: String,
}
