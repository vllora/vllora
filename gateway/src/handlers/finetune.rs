use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use vllora_core::credentials::KeyStorage;
use vllora_core::credentials::ProviderCredentialsId;
use vllora_core::metadata::models::finetune_job::DbNewFinetuneJob;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::finetune_job::FinetuneJobService;
use vllora_core::GatewayApiError;
use vllora_finetune::ReinforcementTrainingConfig;
use vllora_finetune::{
    CreateDeploymentRequest, CreateReinforcementFinetuningJobRequest, LangdbCloudFinetuneClient,
};
use vllora_llm::types::credentials::Credentials;

#[derive(Debug, Deserialize)]
pub struct ReinforcementJobQuery {
    pub limit: Option<u32>,
    pub after: Option<String>,
    pub dataset_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReinforcementJobStatusResponse {
    pub provider_job_id: String,
    pub status: String,
    pub fine_tuned_model: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FinetuningJobResponse {
    pub id: String,
    pub provider_job_id: String,
    pub status: String,
    pub base_model: String,
    pub fine_tuned_model: Option<String>,
    pub provider: String,
    pub training_config: Option<ReinforcementTrainingConfig>,
    pub suffix: Option<String>,
    pub error_message: Option<String>,
    pub training_file_id: String,
    pub validation_file_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

pub async fn get_langdb_api_key(
    key_storage: &dyn KeyStorage,
    project_slug: Option<&str>,
) -> Result<String, GatewayApiError> {
    let credentials = key_storage
        .get_key(ProviderCredentialsId::new(
            "default".to_string(),
            "langdb".to_string(),
            project_slug.map(|s| s.to_string()),
        ))
        .await
        .map_err(|e| GatewayApiError::CustomError(format!("Failed to get credentials: {}", e)))?;

    if let Some(Ok(c)) = credentials.map(|c| serde_json::from_str::<Credentials>(&c)) {
        match c {
            Credentials::ApiKey(key) => Ok(key.api_key),
            _ => Err(GatewayApiError::CustomError(
                "Invalid credentials".to_string(),
            )),
        }
    } else {
        std::env::var("LANGDB_API_KEY")
            .map_err(|e| GatewayApiError::CustomError(format!("LANGDB_API_KEY not set: {}", e)))
    }
}

// ============================================================================
// Reinforcement Fine-Tuning Handlers
// ============================================================================

/// Upload a dataset file (JSONL format) to the provider
/// This forwards the multipart request to the cloud API
pub async fn upload_dataset(
    mut payload: Multipart,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    // Parse multipart form data to get JSONL file
    let mut jsonl_data: Option<Vec<u8>> = None;
    let mut topic_hierarchy: Option<String> = None;

    while let Some(field) = payload.next().await {
        let mut field = field.map_err(|e| {
            actix_web::error::ErrorBadRequest(format!("Failed to parse multipart form: {}", e))
        })?;

        let field_name = field.name();

        match field_name {
            "file" => {
                // Read JSONL file
                let mut bytes = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(|e| {
                        actix_web::error::ErrorBadRequest(format!(
                            "Failed to read file field: {}",
                            e
                        ))
                    })?;
                    bytes.extend_from_slice(&chunk);
                }
                jsonl_data = Some(bytes);
            }
            "topicHierarchy" | "topic_hierarchy" => {
                // Read topic hierarchy config (JSON string)
                let mut bytes = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(|e| {
                        actix_web::error::ErrorBadRequest(format!(
                            "Failed to read topicHierarchy field: {}",
                            e
                        ))
                    })?;
                    bytes.extend_from_slice(&chunk);
                }

                if !bytes.is_empty() {
                    let s = String::from_utf8(bytes).map_err(|e| {
                        actix_web::error::ErrorBadRequest(format!(
                            "topicHierarchy must be UTF-8 encoded JSON: {}",
                            e
                        ))
                    })?;
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        // Validate it's JSON before forwarding
                        serde_json::from_str::<serde_json::Value>(trimmed).map_err(|e| {
                            actix_web::error::ErrorBadRequest(format!(
                                "Invalid topicHierarchy JSON: {}",
                                e
                            ))
                        })?;
                        topic_hierarchy = Some(trimmed.to_string());
                    }
                }
            }
            _ => {
                // Ignore unknown fields
                continue;
            }
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

    // Get API key and create client
    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    // Upload dataset using client
    let response = client
        .upload_dataset(jsonl_data, topic_hierarchy)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to upload dataset: {}", e))
        })?;

    Ok(HttpResponse::Created().json(response))
}

/// Create a reinforcement fine-tuning job
/// This forwards the request to the cloud API and saves to local database
pub async fn create_reinforcement_job(
    request: web::Json<CreateReinforcementFinetuningJobRequest>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    // Extract owned request body once so we can reuse it
    let request_body = request.into_inner();

    // Get API key and create client
    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    // Create job using client (forwards to cloud API)
    let cloud_response = client
        .create_reinforcement_job(request_body.clone())
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to create job: {}", e))
        })?;

    // Save job to local database
    let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());

    let new_job = DbNewFinetuneJob::new(
        project.id.to_string(),
        request_body.dataset.clone(),
        "langdb".to_string(),
        cloud_response.provider_job_id.clone(),
        request_body.base_model.clone(),
    )
    .with_fine_tuned_model(cloud_response.fine_tuned_model.clone())
    .with_training_file_id(Some(request_body.dataset.clone()))
    .with_validation_file_id(request_body.evaluation_dataset.clone())
    .with_training_config(request_body.training_config.clone());

    // Save to database (ignore errors - job is already created in cloud)
    if let Err(e) = finetune_job_service.create(new_job) {
        tracing::warn!("Failed to save finetune job to local database: {}", e);
    }

    Ok(HttpResponse::Created().json(cloud_response))
}

/// Get reinforcement fine-tuning job status
/// Queries local database first, falls back to cloud API if not found
pub async fn get_reinforcement_job_status(
    job_id: web::Path<String>,
    _query: web::Query<ReinforcementJobQuery>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let job_id_str = job_id.into_inner();

    // Try to find job in local database
    let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());

    if let Ok(Some(db_job)) =
        finetune_job_service.get_by_provider_job_id(&job_id_str, &project.id.to_string())
    {
        return Ok(HttpResponse::Ok().json(ReinforcementJobStatusResponse {
            provider_job_id: db_job.provider_job_id,
            status: db_job.state,
            fine_tuned_model: db_job.fine_tuned_model,
            error_message: db_job.error_message,
        }));
    }

    // If not found in database, query cloud API (backward compatibility)
    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    // Get job status using client
    let response = client
        .get_reinforcement_job_status(&job_id_str)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to get job status: {}", e))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

/// List reinforcement fine-tuning jobs
/// Queries local database
pub async fn list_reinforcement_jobs(
    query: web::Query<ReinforcementJobQuery>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    // Query local database
    let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());

    let db_jobs = finetune_job_service
        .list_by_project(
            &project.id.to_string(),
            query.limit,
            query.after.as_deref(),
            query.dataset_id.as_deref(),
        )
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to list jobs: {}", e))
        })?;

    let response: Vec<FinetuningJobResponse> = db_jobs
        .into_iter()
        .map(|job| FinetuningJobResponse {
            id: job.id,
            provider_job_id: job.provider_job_id,
            status: job.state,
            base_model: job.base_model,
            fine_tuned_model: job.fine_tuned_model,
            provider: job.provider,
            training_config: job
                .training_config
                .and_then(|tc| serde_json::from_str(&tc).ok()),
            suffix: None,
            error_message: job.error_message,
            training_file_id: job.training_file_id.unwrap_or_default(),
            validation_file_id: job.validation_file_id,
            created_at: job.created_at,
            updated_at: job.updated_at,
            completed_at: job.completed_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(response))
}

// ============================================================================
// Deployment Handlers
// ============================================================================

/// Deploy a fine-tuned model
/// This forwards the request to the cloud API
pub async fn deploy_model(
    request: web::Json<CreateDeploymentRequest>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    // Get API key and create client
    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    // Deploy model using client
    let response = client
        .deploy_model(request.into_inner())
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to deploy model: {}", e))
        })?;

    Ok(HttpResponse::Created().json(response))
}

/// Delete a deployment
/// This forwards the request to the cloud API
pub async fn delete_deployment(
    deployment_id: web::Path<String>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let deployment_id_str = deployment_id.into_inner();

    // Get API key and create client
    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    // Delete deployment using client
    client
        .delete_deployment(&deployment_id_str)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to delete deployment: {}",
                e
            ))
        })?;

    Ok(HttpResponse::NoContent().finish())
}
