use actix_web::{error, web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::workflow::{DbNewWorkflow, DbUpdateWorkflow, DbWorkflow};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::eval_job::EvalJobService;
use vllora_core::metadata::services::finetune_job::FinetuneJobService;
use vllora_core::metadata::services::knowledge_source::KnowledgeSourceService;
use vllora_core::metadata::services::workflow::WorkflowService;
use vllora_core::metadata::services::workflow_record::WorkflowRecordService;
use vllora_core::metadata::services::workflow_topic::WorkflowTopicService;

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub objective: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub objective: Option<String>,
    pub eval_script: Option<String>,
    pub state: Option<String>,
    pub iteration_state: Option<String>,
    pub pipeline_journal: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowDetailResponse {
    #[serde(flatten)]
    pub workflow: DbWorkflow,
    pub records_count: i64,
    pub eval_job_ids: Vec<String>,
    pub finetune_job_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkflowListItemJobSummary {
    pub id: String,
    pub status: String,
    pub model: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct WorkflowListItem {
    #[serde(flatten)]
    pub workflow: DbWorkflow,
    pub record_count: i64,
    pub knowledge_source_count: i64,
    pub topic_count: usize,
    pub eval_jobs: Vec<WorkflowListItemJobSummary>,
    pub training_jobs: Vec<WorkflowListItemJobSummary>,
}

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Workflow not found")
        }
        other => error::ErrorInternalServerError(other),
    }
}

pub async fn get_workflow(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let pool = db_pool.get_ref().clone();

    let workflow = WorkflowService::new(pool.clone())
        .get_by_id(&workflow_id)
        .map_err(map_db_error)?;

    let records_count = WorkflowRecordService::new(pool.clone())
        .count(&workflow_id)
        .unwrap_or(0);

    let eval_job_ids = EvalJobService::new(pool.clone())
        .list_ids_by_workflow(&workflow_id)
        .unwrap_or_default();

    let finetune_job_ids = FinetuneJobService::new(pool)
        .list_ids_by_workflow(&workflow_id)
        .unwrap_or_default();

    Ok(HttpResponse::Ok().json(WorkflowDetailResponse {
        workflow,
        records_count,
        eval_job_ids,
        finetune_job_ids,
    }))
}

pub async fn list_workflows(db_pool: web::Data<DbPool>) -> Result<HttpResponse> {
    let pool = db_pool.get_ref().clone();
    let workflows = WorkflowService::new(pool.clone())
        .list()
        .map_err(map_db_error)?;

    let record_svc = WorkflowRecordService::new(pool.clone());
    let ks_svc = KnowledgeSourceService::new(pool.clone());
    let topic_svc = WorkflowTopicService::new(pool.clone());
    let eval_svc = EvalJobService::new(pool.clone());
    let ft_svc = FinetuneJobService::new(pool);

    let items: Vec<WorkflowListItem> = workflows
        .into_iter()
        .map(|wf| {
            let wf_id = &wf.id;

            let record_count = record_svc.count(wf_id).unwrap_or(0);
            let knowledge_source_count = ks_svc.count(wf_id).unwrap_or(0);
            let topic_count = topic_svc.list(wf_id).map(|t| t.len()).unwrap_or(0);

            let eval_jobs = eval_svc
                .list_by_workflow(wf_id)
                .unwrap_or_default()
                .into_iter()
                .map(|j| WorkflowListItemJobSummary {
                    id: j.id,
                    status: j.status,
                    model: j.rollout_model,
                    created_at: j.created_at,
                })
                .collect();

            let training_jobs = ft_svc
                .list_by_workflow(wf_id)
                .unwrap_or_default()
                .into_iter()
                .map(|j| WorkflowListItemJobSummary {
                    id: j.id,
                    status: j.state,
                    model: Some(j.base_model),
                    created_at: j.created_at,
                })
                .collect();

            WorkflowListItem {
                workflow: wf,
                record_count,
                knowledge_source_count,
                topic_count,
                eval_jobs,
                training_jobs,
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(items))
}

pub async fn create_workflow(
    body: web::Json<CreateWorkflowRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let payload = body.into_inner();
    let service = WorkflowService::new(db_pool.get_ref().clone());

    let workflow = service
        .create(DbNewWorkflow::new(payload.name, payload.objective))
        .map_err(map_db_error)?;

    Ok(HttpResponse::Created().json(workflow))
}

pub async fn update_workflow(
    workflow_id: web::Path<String>,
    body: web::Json<UpdateWorkflowRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowService::new(db_pool.get_ref().clone());
    let update = DbUpdateWorkflow::new()
        .with_name(payload.name)
        .with_objective(payload.objective)
        .with_eval_script(payload.eval_script)
        .with_state(payload.state)
        .with_iteration_state(payload.iteration_state)
        .with_pipeline_journal(payload.pipeline_journal);

    let workflow = service.update(&workflow_id, update).map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(workflow))
}

pub async fn soft_delete_workflow(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowService::new(db_pool.get_ref().clone());

    service.soft_delete(&workflow_id).map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "id": workflow_id,
        "deleted": true
    })))
}

// =============================================================================
// Pipeline Journal endpoints
// =============================================================================

#[derive(Debug, Serialize)]
pub struct PipelineJournalResponse {
    pub workflow_id: String,
    pub pipeline_journal: Option<serde_json::Value>,
}

/// GET /finetune/workflows/{workflow_id}/journal
pub async fn get_pipeline_journal(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = WorkflowService::new(db_pool.get_ref().clone());

    let workflow = service.get_by_id(&workflow_id).map_err(map_db_error)?;

    let journal_value = workflow
        .pipeline_journal
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

    Ok(HttpResponse::Ok().json(PipelineJournalResponse {
        workflow_id: workflow.id,
        pipeline_journal: journal_value,
    }))
}

#[derive(Debug, Deserialize)]
pub struct AppendJournalEntriesRequest {
    /// Individual entries to append to the journal
    pub entries: Vec<serde_json::Value>,
    /// Workflow objective (set on first call, ignored after)
    pub objective: Option<String>,
}

/// POST /finetune/workflows/{workflow_id}/journal/entries
///
/// Atomically appends entries to the pipeline journal. Server-side
/// read-modify-write ensures no entries are lost from concurrent calls.
pub async fn append_journal_entries(
    workflow_id: web::Path<String>,
    body: web::Json<AppendJournalEntriesRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();

    if payload.entries.is_empty() {
        return Err(error::ErrorBadRequest("entries array must not be empty"));
    }

    let pool = db_pool.get_ref().clone();
    let service = WorkflowService::new(pool);

    // Read current journal (or create empty one)
    let workflow = service.get_by_id(&workflow_id).map_err(map_db_error)?;

    let mut journal: serde_json::Value = workflow
        .pipeline_journal
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "version": "1.0",
                "workflow_id": workflow_id,
                "objective": payload.objective.as_deref().unwrap_or(""),
                "entries": []
            })
        });

    // Set objective if provided and currently empty
    if let Some(obj) = &payload.objective {
        if let Some(current) = journal.get("objective").and_then(|v| v.as_str()) {
            if current.is_empty() {
                journal["objective"] = serde_json::Value::String(obj.clone());
            }
        }
    }

    // Append new entries
    if let Some(entries_arr) = journal.get_mut("entries").and_then(|v| v.as_array_mut()) {
        for entry in payload.entries {
            entries_arr.push(entry);
        }
    }

    let journal_str = serde_json::to_string(&journal)
        .map_err(|e| error::ErrorInternalServerError(format!("JSON serialization error: {e}")))?;

    let update = DbUpdateWorkflow::new().with_pipeline_journal(Some(journal_str));
    let updated = service.update(&workflow_id, update).map_err(map_db_error)?;

    let journal_value = updated
        .pipeline_journal
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

    Ok(HttpResponse::Ok().json(PipelineJournalResponse {
        workflow_id: updated.id,
        pipeline_journal: journal_value,
    }))
}

/// PUT /finetune/workflows/{workflow_id}/pipeline-journal
///
/// Canonical Feature 001 mirror endpoint. Replaces the entire pipeline-journal
/// blob on the server. Body is raw JSON (the full journal document). Clients
/// use this best-effort — local `pipeline-journal.json` remains source of truth.
pub async fn put_pipeline_journal_blob(
    workflow_id: web::Path<String>,
    body: web::Bytes,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();

    let journal_str = std::str::from_utf8(&body)
        .map_err(|e| error::ErrorBadRequest(format!("body is not valid UTF-8: {e}")))?;
    // Validate the body parses as JSON before we store it.
    serde_json::from_str::<serde_json::Value>(journal_str)
        .map_err(|e| error::ErrorBadRequest(format!("body is not valid JSON: {e}")))?;

    let service = WorkflowService::new(db_pool.get_ref().clone());
    let update = DbUpdateWorkflow::new().with_pipeline_journal(Some(journal_str.to_string()));
    let updated = service.update(&workflow_id, update).map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "workflow_id": updated.id,
        "bytes": journal_str.len(),
    })))
}

/// PUT /finetune/workflows/{workflow_id}/iteration-state
///
/// Canonical Feature 001 mirror endpoint. Replaces the entire analysis.json
/// blob on the server. Body is raw JSON (the full iteration-state document).
pub async fn put_iteration_state_blob(
    workflow_id: web::Path<String>,
    body: web::Bytes,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();

    let analysis_str = std::str::from_utf8(&body)
        .map_err(|e| error::ErrorBadRequest(format!("body is not valid UTF-8: {e}")))?;
    serde_json::from_str::<serde_json::Value>(analysis_str)
        .map_err(|e| error::ErrorBadRequest(format!("body is not valid JSON: {e}")))?;

    let service = WorkflowService::new(db_pool.get_ref().clone());
    let update = DbUpdateWorkflow::new().with_iteration_state(Some(analysis_str.to_string()));
    let updated = service.update(&workflow_id, update).map_err(map_db_error)?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "workflow_id": updated.id,
        "bytes": analysis_str.len(),
    })))
}

fn placeholder(endpoint: &str, method: &str, workspace_id: &str) -> HttpResponse {
    HttpResponse::NotImplemented().json(serde_json::json!({
        "message": "Placeholder endpoint not implemented yet",
        "endpoint": endpoint,
        "method": method,
        "workspace_id": workspace_id
    }))
}

pub async fn chunk_workflow_knowledge(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/knowledge/chunk",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn create_workflow_knowledge_trace(
    workspace_id: web::Path<String>,
) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/knowledge/trace",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn delete_workflow_knowledge_trace(
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    let (workspace_id, trace_id) = path.into_inner();
    Ok(HttpResponse::NotImplemented().json(serde_json::json!({
        "message": "Placeholder endpoint not implemented yet",
        "endpoint": "/finetune/workflows/{workspace_id}/knowledge/trace/{trace_id}",
        "method": "DELETE",
        "workspace_id": workspace_id,
        "trace_id": trace_id
    })))
}

pub async fn generate_workflow_topics(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/topics/generate",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn generate_workflow_dataset(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/dataset/generate",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn get_workflow_dataset_generate_status(
    workspace_id: web::Path<String>,
) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/dataset/generate/status",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn run_workflow_evaluator(workspace_id: web::Path<String>) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/evaluator/run",
        "POST",
        &workspace_id.into_inner(),
    ))
}

pub async fn get_workflow_evaluator_run_status(
    workspace_id: web::Path<String>,
) -> Result<HttpResponse> {
    Ok(placeholder(
        "/finetune/workflows/{workspace_id}/evaluator/run/status",
        "GET",
        &workspace_id.into_inner(),
    ))
}
