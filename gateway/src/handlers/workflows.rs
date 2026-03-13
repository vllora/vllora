use actix_web::{error, web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::workflow::{DbNewWorkflow, DbUpdateWorkflow, DbWorkflow};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::eval_job::EvalJobService;
use vllora_core::metadata::services::finetune_job::FinetuneJobService;
use vllora_core::metadata::services::workflow::WorkflowService;
use vllora_core::metadata::services::workflow_record::WorkflowRecordService;

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
}

#[derive(Debug, Serialize)]
pub struct WorkflowDetailResponse {
    #[serde(flatten)]
    pub workflow: DbWorkflow,
    pub records_count: i64,
    pub eval_job_ids: Vec<String>,
    pub finetune_job_ids: Vec<String>,
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
    let service = WorkflowService::new(db_pool.get_ref().clone());
    let workflows = service.list().map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(workflows))
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
        .with_iteration_state(payload.iteration_state);

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
