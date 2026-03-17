use actix_web::{error, web, HttpResponse, Result};
use serde::Deserialize;
use serde::Serialize;
use vllora_core::metadata::error::DatabaseError;
use vllora_core::metadata::models::workflow_log::DbNewWorkflowLog;
use vllora_core::metadata::models::workflow_log::DbWorkflowLog;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::workflow_log::WorkflowLogService;

fn map_db_error(err: DatabaseError) -> actix_web::Error {
    match err {
        DatabaseError::QueryError(diesel::result::Error::NotFound) => {
            error::ErrorNotFound("Workflow log not found")
        }
        other => error::ErrorInternalServerError(other),
    }
}

#[derive(Debug, Deserialize)]
pub struct WorkflowLogInput {
    pub target: Option<String>,
    pub log: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowLogsRequest {
    pub logs: Vec<WorkflowLogInput>,
}

#[derive(Debug, Deserialize)]
pub struct ListWorkflowLogsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub logs: Vec<DbWorkflowLog>,
}

#[derive(Debug, Serialize)]
pub struct LogsPaginationResponse {
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct LogsListResponse {
    pub logs: Vec<DbWorkflowLog>,
    #[serde(flatten)]
    pub pagination: LogsPaginationResponse,
}

pub async fn create_workflow_logs_bulk(
    workflow_id: web::Path<String>,
    body: web::Json<CreateWorkflowLogsRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let payload = body.into_inner();
    let service = WorkflowLogService::new(db_pool.get_ref().clone());

    if payload.logs.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "logs must contain at least one entry"
        })));
    }

    let new_logs: Vec<DbNewWorkflowLog> = payload
        .logs
        .into_iter()
        .map(|entry| DbNewWorkflowLog::new(workflow_id.clone(), entry.target, entry.log))
        .collect();

    let created = service
        .create_bulk(&workflow_id, new_logs)
        .map_err(map_db_error)?;

    let response = LogsResponse { logs: created };
    Ok(HttpResponse::Created().json(response))
}

pub async fn list_workflow_logs(
    workflow_id: web::Path<String>,
    query: web::Query<ListWorkflowLogsQuery>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let workflow_id = workflow_id.into_inner();
    let query = query.into_inner();
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let service = WorkflowLogService::new(db_pool.get_ref().clone());
    let total = service
        .count_by_workflow(&workflow_id)
        .map_err(map_db_error)?;
    let logs = service
        .list_by_workflow(&workflow_id, limit, offset)
        .map_err(map_db_error)?;
    let has_more = offset + (logs.len() as i64) < total;

    let response = LogsListResponse {
        logs,
        pagination: LogsPaginationResponse {
            total,
            limit,
            offset,
            has_more,
        },
    };
    Ok(HttpResponse::Ok().json(response))
}
