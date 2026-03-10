use actix_web::{error, web, HttpResponse, Result};
use serde::Deserialize;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::workflow::{DbNewWorkflow, DbUpdateWorkflow};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::workflow::WorkflowService;

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub objective: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub objective: Option<String>,
}

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Workflow not found")
        }
        other => error::ErrorInternalServerError(other),
    }
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
        .with_objective(payload.objective);

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
