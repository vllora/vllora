use actix_web::{error, web, HttpResponse, Result};
use serde::Deserialize;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::eval_job::DbUpdateEvalJob;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::eval_job::EvalJobService;

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Eval job not found")
        }
        other => error::ErrorInternalServerError(other),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateEvalJobRequest {
    pub cloud_run_id: Option<String>,
    pub sample_size: Option<i32>,
    pub rollout_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEvalJobRequest {
    pub status: Option<String>,
    pub error: Option<String>,
    pub completed_at: Option<String>,
    pub started_at: Option<String>,
    pub polling_snapshot: Option<String>,
    pub result: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StatusQuery {
    pub status: String,
}

pub async fn create_eval_job(
    workflow_id: web::Path<String>,
    body: web::Json<CreateEvalJobRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = EvalJobService::new(db_pool.get_ref().clone());

    let job = service
        .create(
            &workflow_id,
            payload.cloud_run_id.as_deref(),
            payload.sample_size,
            payload.rollout_model.as_deref(),
        )
        .map_err(map_db_error)?;
    Ok(HttpResponse::Created().json(job))
}

pub async fn get_eval_job(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, job_id) = path.into_inner();
    let service = EvalJobService::new(db_pool.get_ref().clone());

    let job = service.get(&job_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(job))
}

pub async fn list_eval_jobs(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = EvalJobService::new(db_pool.get_ref().clone());

    let jobs = service
        .list_by_workflow(&workflow_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "jobs": jobs })))
}

pub async fn list_eval_jobs_by_status(
    query: web::Query<StatusQuery>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let query = query.into_inner();
    let service = EvalJobService::new(db_pool.get_ref().clone());

    let jobs = service
        .list_by_status(&query.status)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "jobs": jobs })))
}

pub async fn update_eval_job(
    path: web::Path<(String, String)>,
    body: web::Json<UpdateEvalJobRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, job_id) = path.into_inner();
    let payload = body.into_inner();
    let service = EvalJobService::new(db_pool.get_ref().clone());

    let changeset = DbUpdateEvalJob::with_full_update(
        payload.status,
        payload.error,
        payload.completed_at,
        payload.started_at,
        payload.polling_snapshot,
        payload.result,
    );

    let job = service
        .update_full(&job_id, changeset)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(job))
}

pub async fn delete_eval_job(
    path: web::Path<(String, String)>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let (_workflow_id, job_id) = path.into_inner();
    let service = EvalJobService::new(db_pool.get_ref().clone());

    service.delete(&job_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": true })))
}

pub async fn delete_workflow_eval_jobs(
    workflow_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let service = EvalJobService::new(db_pool.get_ref().clone());

    let count = service
        .delete_by_workflow(&workflow_id)
        .map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "deleted": count })))
}
