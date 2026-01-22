use crate::types::{
    CreateDeploymentRequest, CreateReinforcementFinetuningJobRequest, DeploymentResponse,
    FinetuningJobResponse, FinetuningJobResult, ReinforcementJobStatusResponse,
    UploadDatasetResponse,
};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};

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
        topic_hierarchy: Option<String>,
        evaluator: Option<String>,
    ) -> Result<UploadDatasetResponse, String> {
        let mut form = reqwest::multipart::Form::new();

        let jsonl_part = reqwest::multipart::Part::bytes(jsonl_data)
            .file_name("training.jsonl")
            .mime_str("application/x-ndjson")
            .map_err(|e| format!("Failed to create multipart part: {}", e))?;
        form = form.part("file", jsonl_part);

        if let Some(topic_hierarchy) = topic_hierarchy {
            let topic_part = reqwest::multipart::Part::text(topic_hierarchy)
                .mime_str("application/json")
                .map_err(|e| format!("Failed to create topic_hierarchy part: {}", e))?;
            form = form.part("topic_hierarchy", topic_part);
        }

        if let Some(evaluator) = evaluator {
            let evaluator_part = reqwest::multipart::Part::text(evaluator)
                .mime_str("application/json")
                .map_err(|e| format!("Failed to create evaluator part: {}", e))?;
            form = form.part("evaluator", evaluator_part);
        }

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

        let text = response
            .text()
            .await
            .map_err(|e| format!("Failed to get response: {}", e))?;

        let body: FinetuningJobResult = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse response: {}. Response: {}", e, text))?;

        match body {
            FinetuningJobResult::Success(response) => Ok(*response),
            FinetuningJobResult::Error(error) => {
                Err(format!("Failed to create job: {}", error.message))
            }
        }
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
