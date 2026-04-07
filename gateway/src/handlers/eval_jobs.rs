use actix_web::{error, web, HttpResponse, Result};
use serde::Deserialize;
use vllora_core::metadata::error::DatabaseError;
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
pub struct StatusQuery {
    pub status: String,
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

// ─── Non-scoped handlers (no workflow_id required) ───────────────────────────
// Used by FE when only the job_id is known.

pub async fn get_eval_job_by_id(
    job_id: web::Path<String>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let service = EvalJobService::new(db_pool.get_ref().clone());
    let job = service.get(&job_id).map_err(map_db_error)?;
    Ok(HttpResponse::Ok().json(job))
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
