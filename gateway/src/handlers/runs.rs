use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, Result};
use langdb_metadata::models::project::DbProject;
use langdb_metadata::models::run::RunUsageResponse;
use langdb_metadata::pool::DbPool;
use langdb_metadata::services::run::{ListRunsQuery, RunService, RunServiceImpl};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ListRunsQueryParams {
    pub run_ids: Option<String>,        // Comma-separated
    pub thread_ids: Option<String>,     // Comma-separated
    pub trace_ids: Option<String>,      // Comma-separated
    pub model_name: Option<String>,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct PaginatedResult<T> {
    pub pagination: Pagination,
    pub data: Vec<T>,
}

#[derive(Serialize)]
pub struct Pagination {
    pub offset: i64,
    pub limit: i64,
    pub total: i64,
}

pub async fn list_runs(
    req: HttpRequest,
    query: web::Query<ListRunsQueryParams>,
    db_pool: web::Data<Arc<DbPool>>,
) -> Result<HttpResponse> {
    let run_service = RunServiceImpl::new(db_pool.get_ref().clone());

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_id = req.extensions().get::<DbProject>().map(|p| p.id.clone());

    let list_query = ListRunsQuery {
        project_id,
        run_ids: query
            .run_ids
            .as_ref()
            .map(|s| s.split(',').map(String::from).collect()),
        thread_ids: query
            .thread_ids
            .as_ref()
            .map(|s| s.split(',').map(String::from).collect()),
        trace_ids: query
            .trace_ids
            .as_ref()
            .map(|s| s.split(',').map(String::from).collect()),
        model_name: query.model_name.clone(),
        start_time_min: query.start_time_min,
        start_time_max: query.start_time_max,
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
    };

    match run_service.list(list_query.clone()) {
        Ok(runs) => {
            let total = run_service.count(list_query).unwrap_or(0);

            // Convert to RunUsageResponse for JSON serialization
            let runs_response: Vec<RunUsageResponse> = runs
                .into_iter()
                .map(|run| run.into())
                .collect();

            let result = PaginatedResult {
                pagination: Pagination {
                    offset: query.offset.unwrap_or(0),
                    limit: query.limit.unwrap_or(100),
                    total,
                },
                data: runs_response,
            };

            Ok(HttpResponse::Ok().json(result))
        }
        Err(e) => {
            eprintln!("Error listing runs: {:?}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to list runs"
            })))
        }
    }
}
