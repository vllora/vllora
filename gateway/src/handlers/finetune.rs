use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use vllora_core::credentials::KeyStorage;
use vllora_core::credentials::ProviderCredentialsId;
use vllora_core::executor::context::ExecutorContext;
use vllora_core::handler::CallbackHandlerFn;
use vllora_core::metadata::models::finetune_job::DbNewFinetuneJob;
use vllora_core::metadata::models::workflow::DbUpdateWorkflow;
use vllora_core::metadata::models::workflow_topic::DbWorkflowTopic;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::eval_job::EvalJobService;
use vllora_core::metadata::services::finetune_job::FinetuneJobService;
use vllora_core::metadata::services::workflow::WorkflowService;
use vllora_core::metadata::services::workflow_record::WorkflowRecordService;
use vllora_core::metadata::services::workflow_topic::WorkflowTopicService;
use vllora_core::model::DefaultModelMetadataFactory;
use vllora_core::model::ModelMetadataFactory;
use vllora_core::routing::interceptor::rate_limiter::InMemoryRateLimiterService;
use vllora_core::types::guardrails::service::GuardrailsEvaluator;
use vllora_core::types::metadata::project::Project;
use vllora_core::types::metadata::services::model::ModelService;
use vllora_core::GatewayApiError;
use vllora_finetune::types::{
    CreateEvaluationRequest, CreateJobRequest, DatasetAnalyticsResponse,
    DryRunDatasetAnalyticsRequest, DryRunDatasetAnalyticsResponse, DryRunEvaluatorRequest,
    EvaluationResultQuery, EvaluationResultResponse, Evaluator, FinetuneEvalResultsResponse,
    FinetuneJobMetricsResponse, FinetuneJobQuery, FinetuneTrainingConfig, JobType,
    UpdateEvaluatorResponse,
};
use vllora_finetune::{
    CreateDeploymentRequest, CreateFinetuneJobRequest, LangdbCloudFinetuneClient,
};
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::gateway::ChatCompletionMessage;
use vllora_llm::types::gateway::CostCalculator;

/// Response type for list_finetune_jobs which reads from local SQLite DB.
/// Uses String types to match the SQLite model without conversion overhead.
#[derive(Debug, Serialize)]
pub struct LocalFinetuningJobResponse {
    pub id: String,
    pub provider_job_id: String,
    pub workflow_id: String,
    pub evaluator_version: Option<i32>,
    pub status: String,
    pub base_model: String,
    pub fine_tuned_model: Option<String>,
    pub provider: String,
    pub training_config: Option<FinetuneTrainingConfig>,
    pub suffix: Option<String>,
    pub error_message: Option<String>,
    pub training_file_id: String,
    pub validation_file_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

/// Response type for get_finetune_job_status from local SQLite DB.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct LocalFinetuneJobStatusResponse {
    pub job_id: uuid::Uuid,
    pub provider_job_id: String,
    pub status: String,
    pub fine_tuned_model: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JobRequestPath {
    #[allow(dead_code)]
    pub workflow_id: uuid::Uuid,
    pub job_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
pub struct JobIdPath {
    pub job_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
pub struct GetJobStatusQuery {
    pub job_type: Option<JobType>,
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
// Evaluation Handlers
// ============================================================================

pub async fn create_evaluation(
    request: web::Json<CreateEvaluationRequest>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let request_body = request.into_inner();
    let workflow_id = request_body.dataset_id.to_string();
    let sample_size = request_body.limit;
    let rollout_model = request_body.rollout_model_params.model.clone();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    // Auto-upload dataset from local SQLite to cloud (upsert)
    ensure_dataset_and_evaluator_uploaded(request_body.dataset_id, &client, db_pool.get_ref())
        .await?;

    let cloud_request = CreateEvaluationRequest {
        dataset_id: request_body.dataset_id,
        rollout_model_params: request_body.rollout_model_params,
        offset: request_body.offset,
        limit: request_body.limit,
    };

    let response = client.create_evaluation(cloud_request).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!(
            "Failed to create evaluation run: {}",
            e
        ))
    })?;

    // Create tracking record in SQLite with cloud_run_id already set
    let eval_job_service = EvalJobService::new(db_pool.get_ref().clone());
    let eval_job = eval_job_service
        .create(
            &workflow_id,
            Some(&response.evaluation_run_id.to_string()),
            sample_size,
            rollout_model.as_deref(),
        )
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to save eval job to local database: {}",
                e
            ))
        })?;

    // Update status to running since cloud accepted it
    let _ = eval_job_service.update_full(
        &eval_job.id,
        vllora_core::metadata::models::eval_job::DbUpdateEvalJob::with_status(
            "running".to_string(),
        ),
    );

    Ok(HttpResponse::Created().json(response))
}

pub async fn create_job(
    workflow_id: web::Path<uuid::Uuid>,
    request: web::Json<CreateJobRequest>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let request_body = request.into_inner();
    if let Some(ip) = &request_body.inference_parameters {
        if let Some(reasoning_effort) = ip.reasoning_effort.as_deref() {
            if reasoning_effort.trim().is_empty() {
                return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                    "error": "inference_parameters.reasoning_effort cannot be empty"
                })));
            }
        }
    }

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    ensure_dataset_and_evaluator_uploaded(workflow_id, &client, db_pool.get_ref()).await?;

    let cloud_response = client
        .create_job(&workflow_id, request_body.clone())
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to create job: {}", e))
        })?;

    // Persist provider finetune jobs locally for state-tracker/observability.
    if request_body.job_type == JobType::ProviderFinetune {
        let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());
        let new_job = DbNewFinetuneJob::new(
            project.id.to_string(),
            workflow_id.to_string(),
            "langdb".to_string(),
            cloud_response.job_id.to_string(),
            request_body.base_model.unwrap_or_default(),
        )
        .with_evaluator_version(request_body.evaluator_version)
        .with_fine_tuned_model(request_body.output_model)
        .with_training_file_id(Some(workflow_id.to_string()))
        .with_validation_file_id(request_body.evaluation_dataset)
        .with_training_config(request_body.training_config);

        if let Err(e) = finetune_job_service.create(new_job) {
            tracing::warn!(
                "Failed to save unified finetune job to local database: {}",
                e
            );
        }
    }

    Ok(HttpResponse::Created().json(cloud_response))
}

pub async fn get_job_status(
    path: web::Path<JobIdPath>,
    query: web::Query<GetJobStatusQuery>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let job_id = path.into_inner().job_id.to_string();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let status_result = if let Some(job_type) = query.into_inner().job_type {
        client.get_job_status(&job_id, job_type).await
    } else {
        // Backward compatible fallback when caller does not pass job_type.
        match client
            .get_job_status(&job_id, JobType::ProviderFinetune)
            .await
        {
            Ok(status) => Ok(status),
            Err(_) => client.get_job_status(&job_id, JobType::EvaluationRun).await,
        }
    };

    let status = status_result.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!(
            "Failed to get unified job status: {}",
            e
        ))
    })?;

    Ok(HttpResponse::Ok().json(status))
}

pub async fn get_dataset_analytics(
    workflow_id: web::Path<uuid::Uuid>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let dataset_id_str = workflow_id.into_inner().to_string();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response: DatasetAnalyticsResponse = client
        .get_dataset_analytics(&dataset_id_str)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to get dataset analytics: {}",
                e
            ))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

pub async fn dry_run_dataset_analytics(
    body: web::Json<DryRunDatasetAnalyticsRequest>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let request_body = body.into_inner();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response: DryRunDatasetAnalyticsResponse = client
        .dry_run_dataset_analytics(request_body)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to compute dry-run dataset analytics: {}",
                e
            ))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

pub async fn dry_run_workflow_evaluator(
    workflow_id: web::Path<uuid::Uuid>,
    body: web::Json<DryRunEvaluatorRequest>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let request_body = body.into_inner();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response = client
        .dry_run_workflow_evaluator(&workflow_id.to_string(), request_body)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to run evaluator dry-run: {}",
                e
            ))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

pub async fn get_evaluation_result(
    evaluation_run_id: web::Path<uuid::Uuid>,
    query: web::Query<EvaluationResultQuery>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let run_id_str = evaluation_run_id.into_inner().to_string();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response: EvaluationResultResponse = client
        .get_evaluation_result(&run_id_str, Some(query.into_inner()))
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to get evaluation result: {}",
                e
            ))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

pub async fn get_finetune_evaluations(
    workflow_id: web::Path<uuid::Uuid>,
    query: web::Query<HashMap<String, String>>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let dataset_id_str = workflow_id.into_inner().to_string();

    let row_index = query.get("row_index").and_then(|s| s.parse::<i32>().ok());
    let epoch = query.get("epoch").and_then(|s| s.parse::<i32>().ok());
    let finetune_job_id = query.get("finetune_job_id").cloned();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response: FinetuneEvalResultsResponse = client
        .get_finetune_evaluations(&dataset_id_str, row_index, epoch, finetune_job_id, true)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to get finetune evaluations: {}",
                e
            ))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

// ============================================================================
// Dataset Evaluator Handlers
// ============================================================================

pub async fn get_workflow_evaluator_versions(
    workflow_id: web::Path<uuid::Uuid>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response = client
        .get_workflow_evaluator_versions(&workflow_id.to_string())
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to get evaluator versions: {}",
                e
            ))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

pub async fn update_workflow_evaluator(
    workflow_id: web::Path<uuid::Uuid>,
    mut payload: Multipart,
    _project: web::ReqData<vllora_core::types::metadata::project::Project>,
    _key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let mut script_bytes: Option<Vec<u8>> = None;

    while let Some(field) = payload.next().await {
        let mut field = field.map_err(|e| {
            actix_web::error::ErrorBadRequest(format!("Invalid multipart field: {e}"))
        })?;
        match field.name() {
            "file" => {
                let mut bytes = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_err(|e| {
                        actix_web::error::ErrorBadRequest(format!(
                            "Failed to read evaluator file chunk: {e}"
                        ))
                    })?;
                    bytes.extend_from_slice(&chunk);
                }
                script_bytes = Some(bytes);
            }
            _ => {
                // Drain unknown fields to keep multipart parser state consistent.
                while let Some(chunk) = field.next().await {
                    let _ = chunk.map_err(|e| {
                        actix_web::error::ErrorBadRequest(format!(
                            "Failed to read multipart field chunk: {e}"
                        ))
                    })?;
                }
            }
        }
    }

    let script_bytes = script_bytes.filter(|b| !b.is_empty()).ok_or_else(|| {
        actix_web::error::ErrorBadRequest("Missing evaluator file in 'file' field")
    })?;
    let raw_script = String::from_utf8(script_bytes).map_err(|e| {
        actix_web::error::ErrorBadRequest(format!("Evaluator file must be valid UTF-8: {e}"))
    })?;
    if raw_script.trim().is_empty() {
        return Err(actix_web::error::ErrorBadRequest(
            "Evaluator script file is empty",
        ));
    }

    // Validate by constructing JS evaluator config from uploaded file content.
    let validation_value = serde_json::json!({
        "type": "js",
        "config": {
            "script": format!("__inline_script:{raw_script}")
        }
    });
    let validation_str = serde_json::to_string(&validation_value)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid evaluator JSON: {e}")))?;
    serde_json::from_str::<Evaluator<ChatCompletionMessage>>(&validation_str)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid evaluator format: {e}")))?;

    let workflow_service = WorkflowService::new(db_pool.get_ref().clone());
    workflow_service.update(
        &workflow_id.to_string(),
        DbUpdateWorkflow::new().with_eval_script(Some(raw_script)),
    )?;

    Ok(HttpResponse::Ok().json(UpdateEvaluatorResponse {
        workflow_id,
        updated: true,
    }))
}

// NOTE: upload_dataset was removed — gateway auto-uploads via ensure_dataset_and_evaluator_uploaded()

// ============================================================================
// Workflow Dataset Upload (reads from local SQLite, upserts to cloud)
// ============================================================================

/// Build a topic hierarchy tree from flat DB rows.
/// Returns a JSON array of root nodes with nested children.
fn build_topic_hierarchy_json(topics: &[DbWorkflowTopic]) -> serde_json::Value {
    let mut node_map: HashMap<String, serde_json::Value> = HashMap::new();
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut root_ids: Vec<String> = Vec::new();

    // First pass: create all nodes
    for topic in topics {
        let node = serde_json::json!({
            "id": topic.id,
            "reference_id": topic.reference_id,
            "name": topic.name,
            "system_prompt": topic.system_prompt,
        });

        node_map.insert(topic.id.clone(), node);

        if let Some(ref parent_id) = topic.parent_id {
            children_map
                .entry(parent_id.clone())
                .or_default()
                .push(topic.id.clone());
        } else {
            root_ids.push(topic.id.clone());
        }
    }

    // Recursive helper to build tree from leaves up
    fn build_node(
        id: &str,
        node_map: &HashMap<String, serde_json::Value>,
        children_map: &HashMap<String, Vec<String>>,
    ) -> serde_json::Value {
        let mut node = node_map.get(id).cloned().unwrap_or(serde_json::json!({}));
        if let Some(child_ids) = children_map.get(id) {
            let children: Vec<serde_json::Value> = child_ids
                .iter()
                .map(|cid| build_node(cid, node_map, children_map))
                .collect();
            node["children"] = serde_json::json!(children);
        }
        node
    }

    let roots: Vec<serde_json::Value> = root_ids
        .iter()
        .map(|id| build_node(id, &node_map, &children_map))
        .collect();

    serde_json::json!(roots)
}

/// Convert a single record's `data` JSON into OpenAI training format.
/// Returns None if the record cannot be converted.
fn record_to_training_line(record_id: &str, data: &str) -> Option<serde_json::Value> {
    let parsed: serde_json::Value = serde_json::from_str(data).ok()?;

    let mut row = parsed.as_object()?.clone();
    row.insert("id".to_string(), serde_json::json!(record_id));

    let messages_missing_or_empty = row
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true);

    if messages_missing_or_empty {
        let mut messages: Vec<serde_json::Value> = Vec::new();

        if let Some(input_msgs) = parsed
            .get("input")
            .and_then(|i| i.get("messages"))
            .and_then(|m| m.as_array())
        {
            messages.extend(input_msgs.iter().cloned());
        }

        if let Some(output_msgs) = parsed.get("output").and_then(|o| o.get("messages")) {
            if let Some(arr) = output_msgs.as_array() {
                messages.extend(arr.iter().cloned());
            } else {
                messages.push(output_msgs.clone());
            }
        }

        if messages.is_empty() {
            return None;
        }

        row.insert("messages".to_string(), serde_json::json!(messages));
    }

    if row.get("tool_calls").map(|v| v.is_null()).unwrap_or(true) {
        if let Some(tool_calls) = parsed
            .get("output")
            .and_then(|o| o.get("tool_calls"))
            .or_else(|| parsed.get("input").and_then(|i| i.get("tool_calls")))
        {
            if !tool_calls.is_null() {
                row.insert("tool_calls".to_string(), tool_calls.clone());
            }
        }
    }

    if row.get("ground_truth").map(|v| v.is_null()).unwrap_or(true) {
        if let Some(ground_truth) = parsed
            .get("output")
            .and_then(|o| o.get("ground_truth"))
            .or_else(|| parsed.get("input").and_then(|i| i.get("ground_truth")))
        {
            if !ground_truth.is_null() {
                row.insert("ground_truth".to_string(), ground_truth.clone());
            }
        }
    }

    Some(serde_json::Value::Object(row))
}

/// Read records, topics, and eval_script from local SQLite, build JSONL,
/// and upsert to LangDB Cloud. Returns the cloud dataset ID.
async fn ensure_dataset_and_evaluator_uploaded(
    wf_id: uuid::Uuid,
    client: &LangdbCloudFinetuneClient,
    db_pool: &DbPool,
) -> Result<vllora_finetune::types::UploadDatasetResponse, actix_web::Error> {
    let wf_id_str = wf_id.to_string();

    // Read workflow (for eval_script)
    let workflow_service = WorkflowService::new(db_pool.clone());
    let workflow = workflow_service
        .get_by_id(&wf_id_str)
        .map_err(|e| actix_web::error::ErrorNotFound(format!("Workflow not found: {}", e)))?;

    // Read records
    let record_service = WorkflowRecordService::new(db_pool.clone());
    let records = record_service.list(&wf_id_str).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to read records: {}", e))
    })?;

    if records.is_empty() {
        return Err(actix_web::error::ErrorBadRequest(
            "Workflow has no records to upload",
        ));
    }

    // Build JSONL from records
    let jsonl_lines: Vec<String> = records
        .iter()
        .filter_map(|r| record_to_training_line(&r.id, &r.data).map(|v| v.to_string()))
        .collect();

    if jsonl_lines.is_empty() {
        return Err(actix_web::error::ErrorBadRequest(
            "No valid training records found",
        ));
    }

    let jsonl_data = jsonl_lines.join("\n").into_bytes();

    // Read topics and build hierarchy JSON
    let topic_service = WorkflowTopicService::new(db_pool.clone());
    let topics = topic_service.list(&wf_id_str).unwrap_or_default();
    let topic_hierarchy = if topics.is_empty() {
        None
    } else {
        Some(build_topic_hierarchy_json(&topics).to_string())
    };

    // Upload to cloud (upsert: dataset_id = workflow_id)
    client
        .upload_dataset(
            jsonl_data,
            topic_hierarchy,
            workflow.eval_script,
            Some(wf_id),
        )
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to upload dataset: {}", e))
        })
}

#[allow(dead_code)]
pub async fn create_finetune_job(
    workflow_id: web::Path<uuid::Uuid>,
    request: web::Json<CreateFinetuneJobRequest>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let request_body = request.into_inner();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    // Auto-upload dataset from local SQLite to cloud (upsert)
    ensure_dataset_and_evaluator_uploaded(*workflow_id, &client, db_pool.get_ref()).await?;

    let cloud_response = client
        .create_finetune_job(request_body.clone(), &workflow_id)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to create job: {}", e))
        })?;

    let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());

    let new_job = DbNewFinetuneJob::new(
        project.id.to_string(),
        workflow_id.to_string(),
        "langdb".to_string(),
        cloud_response.id.to_string(),
        request_body.base_model.clone(),
    )
    .with_evaluator_version(request_body.evaluator_version)
    .with_fine_tuned_model(cloud_response.fine_tuned_model.clone())
    .with_training_file_id(Some(workflow_id.to_string()))
    .with_validation_file_id(request_body.evaluation_dataset.clone())
    .with_training_config(request_body.training_config.clone());

    if let Err(e) = finetune_job_service.create(new_job) {
        tracing::warn!("Failed to save finetune job to local database: {}", e);
    }

    Ok(HttpResponse::Created().json(cloud_response))
}

#[allow(dead_code)]
pub async fn get_finetune_job_status(
    path: web::Path<JobRequestPath>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let path = path.into_inner();

    let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());

    if let Ok(Some(db_job)) =
        finetune_job_service.get_by_id(&path.job_id.to_string(), &project.id.to_string())
    {
        return Ok(HttpResponse::Ok().json(LocalFinetuneJobStatusResponse {
            job_id: path.job_id,
            provider_job_id: db_job.provider_job_id,
            status: db_job.state,
            fine_tuned_model: db_job.fine_tuned_model,
            error_message: db_job.error_message,
        }));
    }

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response = client
        .get_finetune_job_status(&path.job_id.to_string())
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to get job status: {}", e))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

pub async fn get_finetune_job_metrics(
    path: web::Path<JobRequestPath>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let path = path.into_inner();

    // Resolve provider_job_id from local DB (FE passes local job UUID)
    let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());
    let provider_job_id = finetune_job_service
        .get_by_id(&path.job_id.to_string(), &project.id.to_string())
        .ok()
        .flatten()
        .map(|db_job| db_job.provider_job_id)
        .unwrap_or_else(|| path.job_id.to_string());

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response: FinetuneJobMetricsResponse = client
        .get_finetune_job_metrics(&provider_job_id)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to get finetune job metrics: {}",
                e
            ))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

pub async fn list_finetune_jobs(
    workflow_id: web::Path<uuid::Uuid>,
    query: web::Query<FinetuneJobQuery>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());

    // Use workflow_id from path param to scope the job listing.
    // Falls back to query.dataset_id for backwards compatibility.
    let filter_workflow_id = Some(workflow_id.to_string());
    let dataset_filter = filter_workflow_id
        .as_deref()
        .or(query.dataset_id.as_deref());

    let db_jobs = finetune_job_service
        .list_by_project(
            &project.id.to_string(),
            query.limit,
            query.after.as_deref(),
            dataset_filter,
        )
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to list jobs: {}", e))
        })?;

    let response: Vec<LocalFinetuningJobResponse> = db_jobs
        .into_iter()
        .map(|job| LocalFinetuningJobResponse {
            id: job.id,
            provider_job_id: job.provider_job_id,
            workflow_id: job.workflow_id,
            evaluator_version: job.evaluator_version,
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

pub async fn cancel_finetune_job(
    path: web::Path<JobRequestPath>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let path = path.into_inner();

    // Resolve provider_job_id from local DB (FE passes local job UUID)
    let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());
    let (local_job_id, provider_job_id) = finetune_job_service
        .get_by_id(&path.job_id.to_string(), &project.id.to_string())
        .ok()
        .flatten()
        .map(|db_job| (Some(db_job.id), db_job.provider_job_id))
        .unwrap_or_else(|| (None, path.job_id.to_string()));

    if let Some(ref id) = local_job_id {
        let _ = finetune_job_service.update_state(
            id,
            &project.id.to_string(),
            vllora_core::metadata::models::finetune_job::FinetuneJobState::Cancelled,
        );
    }

    // Cancel on cloud — retry once on failure since a failed cloud cancel
    // means the training job keeps running and costing money.
    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let mut cloud_cancel_ok = false;
    for attempt in 1..=2 {
        match client.cancel_finetune_job(&provider_job_id).await {
            Ok(()) => {
                cloud_cancel_ok = true;
                break;
            }
            Err(e) => {
                tracing::warn!(
                    "Cloud cancel attempt {}/2 failed for job {}: {}",
                    attempt,
                    provider_job_id,
                    e
                );
            }
        }
    }

    if cloud_cancel_ok {
        Ok(HttpResponse::NoContent().finish())
    } else {
        // Local state is cancelled but cloud cancel failed — return 207 (Multi-Status)
        // so the UI knows the cloud cancel is uncertain and can warn the user.
        Ok(HttpResponse::build(actix_web::http::StatusCode::MULTI_STATUS).json(
            serde_json::json!({
                "local_status": "cancelled",
                "cloud_cancel": "failed",
                "message": "Job marked as cancelled locally, but the cloud training job may still be running. Check the provider dashboard to confirm cancellation."
            }),
        ))
    }
}

pub async fn resume_finetune_job(
    path: web::Path<JobRequestPath>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let path = path.into_inner();

    // Resolve provider_job_id from local DB (FE passes local job UUID)
    let finetune_job_service = FinetuneJobService::new(db_pool.get_ref().clone());
    let (local_job_id, provider_job_id) = finetune_job_service
        .get_by_id(&path.job_id.to_string(), &project.id.to_string())
        .ok()
        .flatten()
        .map(|db_job| (Some(db_job.id), db_job.provider_job_id))
        .unwrap_or_else(|| (None, path.job_id.to_string()));

    if let Some(ref id) = local_job_id {
        let _ = finetune_job_service.update_state(
            id,
            &project.id.to_string(),
            vllora_core::metadata::models::finetune_job::FinetuneJobState::Pending,
        );
    }

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    client
        .resume_finetune_job(&provider_job_id)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to resume finetune job: {}",
                e
            ))
        })?;

    Ok(HttpResponse::NoContent().finish())
}

// ============================================================================
// Weights Download Handler
// ============================================================================

pub async fn get_weights_download_url(
    path: web::Path<JobRequestPath>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let job_id_str = path.job_id.to_string();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response = client
        .get_weights_download_url(&job_id_str)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!(
                "Failed to get weights download URL: {}",
                e
            ))
        })?;

    Ok(HttpResponse::Ok().json(response))
}

// ============================================================================
// Deployment Handlers
// ============================================================================

pub async fn deploy_model(
    request: web::Json<CreateDeploymentRequest>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

    let response = client
        .deploy_model(request.into_inner())
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Failed to deploy model: {}", e))
        })?;

    Ok(HttpResponse::Created().json(response))
}

pub async fn delete_deployment(
    deployment_id: web::Path<String>,
    project: web::ReqData<vllora_core::types::metadata::project::Project>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
) -> Result<HttpResponse> {
    let deployment_id_str = deployment_id.into_inner();

    let api_key = get_langdb_api_key(key_storage.get_ref().as_ref(), Some(&project.slug)).await?;
    let client = LangdbCloudFinetuneClient::new(api_key).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to create client: {}", e))
    })?;

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

// ============================================================================
// Topic Hierarchy Generation Handler
// ============================================================================

pub async fn generate_topic_hierarchy(
    request: web::Json<vllora_core::finetune::GenerateTopicHierarchyProperties>,
    db_pool: web::Data<DbPool>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    project: web::ReqData<Project>,
    cost_calculator: web::Data<Box<dyn CostCalculator>>,
    models_service: web::Data<Box<dyn ModelService>>,
    evaluator_service: web::Data<Box<dyn GuardrailsEvaluator>>,
) -> Result<HttpResponse> {
    let properties = request.into_inner();
    let db_pool = db_pool.get_ref().clone();

    let cb = CallbackHandlerFn(None);
    let cost_calculator = cost_calculator.into_inner();
    let rate_limiter_service = InMemoryRateLimiterService::new();
    let guardrails_evaluator_service = evaluator_service.clone().into_inner();

    let executor_context = ExecutorContext::new(
        cb,
        cost_calculator,
        Arc::new(Box::new(
            DefaultModelMetadataFactory::new(models_service.into_inner()).with_db_pool(&db_pool),
        ) as Box<dyn ModelMetadataFactory>),
        HashMap::new(),
        HashMap::new(),
        guardrails_evaluator_service,
        Arc::new(rate_limiter_service),
        project.id,
        key_storage.into_inner(),
        None,
    );

    let result = vllora_core::finetune::generate_topic_hierarchy(
        properties,
        &executor_context,
        &project.slug,
    )
    .await;

    if result.success {
        Ok(HttpResponse::Ok().json(result))
    } else {
        let error_message = result.error.unwrap_or_else(|| "Unknown error".to_string());
        Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": error_message
        })))
    }
}

pub async fn adjust_topic_hierarchy(
    request: web::Json<vllora_core::finetune::AdjustTopicHierarchyProperties>,
    db_pool: web::Data<DbPool>,
    key_storage: web::Data<Box<dyn KeyStorage>>,
    project: web::ReqData<Project>,
    cost_calculator: web::Data<Box<dyn CostCalculator>>,
    models_service: web::Data<Box<dyn ModelService>>,
    evaluator_service: web::Data<Box<dyn GuardrailsEvaluator>>,
) -> Result<HttpResponse> {
    let properties = request.into_inner();
    let db_pool = db_pool.get_ref().clone();

    let cb = CallbackHandlerFn(None);
    let cost_calculator = cost_calculator.into_inner();
    let rate_limiter_service = InMemoryRateLimiterService::new();
    let guardrails_evaluator_service = evaluator_service.clone().into_inner();

    let executor_context = ExecutorContext::new(
        cb,
        cost_calculator,
        Arc::new(Box::new(
            DefaultModelMetadataFactory::new(models_service.into_inner()).with_db_pool(&db_pool),
        ) as Box<dyn ModelMetadataFactory>),
        HashMap::new(),
        HashMap::new(),
        guardrails_evaluator_service,
        Arc::new(rate_limiter_service),
        project.id,
        key_storage.into_inner(),
        None,
    );

    let result =
        vllora_core::finetune::adjust_topic_hierarchy(properties, &executor_context, &project.slug)
            .await;

    if result.success {
        Ok(HttpResponse::Ok().json(result))
    } else {
        let error_message = result.error.unwrap_or_else(|| "Unknown error".to_string());
        Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": error_message
        })))
    }
}
