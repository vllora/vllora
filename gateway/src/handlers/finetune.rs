use actix_multipart::Multipart;
use actix_web::{web, HttpRequest, HttpResponse, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

/// Get cloud API URL
fn get_api_url() -> String {
    std::env::var("LANGDB_API_URL")
        .unwrap_or_else(|_| vllora_core::types::LANGDB_API_URL.to_string())
}

// ============================================================================
// Reinforcement Fine-Tuning Handlers
// ============================================================================

/// Request to create a reinforcement fine-tuning job
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

/// Upload a dataset file (JSONL format) to the provider
/// This forwards the multipart request to the cloud API
pub async fn upload_dataset(
    mut payload: Multipart,
    req: HttpRequest,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
) -> Result<HttpResponse> {
    let _project_id = project.id;

    // Parse multipart form data to get JSONL file
    let mut jsonl_data: Option<Vec<u8>> = None;

    while let Some(field) = payload.next().await {
        let mut field = field.map_err(|e| {
            actix_web::error::ErrorBadRequest(format!("Failed to parse multipart form: {}", e))
        })?;

        let field_name = field.name();

        if field_name == "file" {
            // Read JSONL file
            let mut bytes = Vec::new();
            while let Some(chunk) = field.next().await {
                let chunk = chunk.map_err(|e| {
                    actix_web::error::ErrorBadRequest(format!("Failed to read file field: {}", e))
                })?;
                bytes.extend_from_slice(&chunk);
            }
            jsonl_data = Some(bytes);
        } else {
            // Ignore unknown fields
            continue;
        }
    }

    let jsonl_data = jsonl_data.ok_or_else(|| {
        actix_web::error::ErrorBadRequest("Missing required field: 'file'".to_string())
    })?;

    if jsonl_data.is_empty() {
        return Err(actix_web::error::ErrorBadRequest(
            "File is empty".to_string(),
        ));
    }

    // Forward multipart request to cloud API
    let client = reqwest::Client::new();
    let mut form = reqwest::multipart::Form::new();

    // Add JSONL file
    let jsonl_part = reqwest::multipart::Part::bytes(jsonl_data)
        .file_name("training.jsonl")
        .mime_str("application/x-ndjson")
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to create multipart part: {}",
                e
            ))
        })?;
    form = form.part("file", jsonl_part);

    // Build request to cloud API
    let mut cloud_request = client
        .post(format!("{}/finetune/datasets", get_api_url()))
        .multipart(form);

    // Add authorization header
    let api_key = std::env::var("LANGDB_API_KEY").unwrap_or_default();
    cloud_request = cloud_request.header("Authorization", api_key);

    // Forward project header if present
    if let Some(project_header) = req.headers().get("x-project-id") {
        if let Ok(project_str) = project_header.to_str() {
            cloud_request = cloud_request.header("x-project-id", project_str);
        }
    }

    // Send request to cloud API
    let response = cloud_request.send().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to call cloud API: {}", e))
    })?;

    // Forward response
    let status_code = actix_web::http::StatusCode::from_u16(response.status().as_u16())
        .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value = response.json().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to read response: {}", e))
    })?;

    Ok(HttpResponse::build(status_code).json(body))
}

/// Create a reinforcement fine-tuning job
/// This forwards the request to the cloud API
pub async fn create_reinforcement_job(
    request: web::Json<CreateReinforcementFinetuningJobRequest>,
    req: HttpRequest,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
) -> Result<HttpResponse> {
    let _project_id = project.id;

    // Forward request to cloud API
    let client = reqwest::Client::new();
    let mut cloud_request = client
        .post(format!("{}/finetune/reinforcement-jobs", get_api_url()))
        .json(&*request);

    // Add authorization header
    let api_key = std::env::var("LANGDB_API_KEY").unwrap_or_default();
    cloud_request = cloud_request.header("Authorization", api_key);

    // Forward project header if present
    if let Some(project_header) = req.headers().get("x-project-id") {
        if let Ok(project_str) = project_header.to_str() {
            cloud_request = cloud_request.header("x-project-id", project_str);
        }
    }

    // Send request to cloud API
    let response = cloud_request.send().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to call cloud API: {}", e))
    })?;

    // Forward response
    let status_code = actix_web::http::StatusCode::from_u16(response.status().as_u16())
        .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value = response.json().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to read response: {}", e))
    })?;

    Ok(HttpResponse::build(status_code).json(body))
}

#[derive(Debug, Deserialize)]
pub struct ReinforcementJobQuery {
    pub limit: Option<u32>,
    pub after: Option<String>,
}

/// Get reinforcement fine-tuning job status
/// This forwards the request to the cloud API
pub async fn get_reinforcement_job_status(
    job_id: web::Path<String>,
    query: web::Query<ReinforcementJobQuery>,
    req: HttpRequest,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
) -> Result<HttpResponse> {
    let _project_id = project.id;
    let job_id_str = job_id.into_inner();

    // Build URL with query parameters
    let mut url = format!(
        "{}/finetune/reinforcement-jobs/{}/status",
        get_api_url(),
        job_id_str
    );
    let mut query_params = Vec::new();
    if let Some(limit) = query.limit {
        query_params.push(format!("limit={}", limit));
    }
    if let Some(after) = &query.after {
        query_params.push(format!("after={}", after));
    }
    if !query_params.is_empty() {
        url.push('?');
        url.push_str(&query_params.join("&"));
    }

    // Forward request to cloud API
    let client = reqwest::Client::new();
    let mut cloud_request = client.get(&url);

    // Add authorization header
    let api_key = std::env::var("LANGDB_API_KEY").unwrap_or_default();
    cloud_request = cloud_request.header("Authorization", api_key);

    // Forward project header if present
    if let Some(project_header) = req.headers().get("x-project-id") {
        if let Ok(project_str) = project_header.to_str() {
            cloud_request = cloud_request.header("x-project-id", project_str);
        }
    }

    // Send request to cloud API
    let response = cloud_request.send().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to call cloud API: {}", e))
    })?;

    // Forward response
    let status_code = actix_web::http::StatusCode::from_u16(response.status().as_u16())
        .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value = response.json().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to read response: {}", e))
    })?;

    Ok(HttpResponse::build(status_code).json(body))
}

/// List reinforcement fine-tuning jobs
/// This forwards the request to the cloud API
pub async fn list_reinforcement_jobs(
    query: web::Query<ReinforcementJobQuery>,
    req: HttpRequest,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
) -> Result<HttpResponse> {
    let _project_id = project.id;

    // Build URL with query parameters
    let mut url = format!("{}/finetune/reinforcement-jobs", get_api_url());
    let mut query_params = Vec::new();
    if let Some(limit) = query.limit {
        query_params.push(format!("limit={}", limit));
    }
    if let Some(after) = &query.after {
        query_params.push(format!("after={}", after));
    }
    if !query_params.is_empty() {
        url.push('?');
        url.push_str(&query_params.join("&"));
    }

    // Forward request to cloud API
    let client = reqwest::Client::new();
    let mut cloud_request = client.get(&url);

    // Add authorization header
    let api_key = std::env::var("LANGDB_API_KEY").unwrap_or_default();
    cloud_request = cloud_request.header("Authorization", api_key);

    // Forward project header if present
    if let Some(project_header) = req.headers().get("x-project-id") {
        if let Ok(project_str) = project_header.to_str() {
            cloud_request = cloud_request.header("x-project-id", project_str);
        }
    }

    // Send request to cloud API
    let response = cloud_request.send().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to call cloud API: {}", e))
    })?;

    // Forward response
    let status_code = actix_web::http::StatusCode::from_u16(response.status().as_u16())
        .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value = response.json().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to read response: {}", e))
    })?;

    Ok(HttpResponse::build(status_code).json(body))
}
