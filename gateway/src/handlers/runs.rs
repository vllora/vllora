use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, Result};
use langdb_core::metadata::models::project::DbProject;
use langdb_core::metadata::models::run::RunUsageResponse;
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::run::{ListRunsQuery, RunService, RunServiceImpl, TypeFilter};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ListRunsQueryParams {
    pub run_ids: Option<String>, // Comma-separated
    #[serde(alias = "threadIds")]
    pub thread_ids: Option<String>, // Comma-separated
    #[serde(alias = "traceIds")]
    pub trace_ids: Option<String>, // Comma-separated
    #[serde(alias = "modelName")]
    pub model_name: Option<String>,
    #[serde(alias = "typeFilter")]
    pub type_filter: Option<TypeFilter>,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    #[serde(alias = "includeMcpTemplates")]
    pub include_mcp_templates: Option<bool>,
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
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let run_service = RunServiceImpl::new(Arc::new(db_pool.get_ref().clone()));

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

    let list_query = ListRunsQuery {
        project_id: project_id.clone(),
        run_ids: query
            .run_ids
            .as_ref()
            .map(|s| s.split(',').map(|id| id.trim().to_string()).collect()),
        thread_ids: query
            .thread_ids
            .as_ref()
            .map(|s| s.split(',').map(|id| id.trim().to_string()).collect()),
        trace_ids: query
            .trace_ids
            .as_ref()
            .map(|s| s.split(',').map(|id| id.trim().to_string()).collect()),
        model_name: query.model_name.clone(),
        type_filter: query.type_filter.clone(),
        start_time_min: query.start_time_min,
        start_time_max: query.start_time_max,
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
        include_mcp_templates: query.include_mcp_templates.unwrap_or(false),
    };

    match run_service.list(list_query.clone()) {
        Ok(runs) => {
            let total = run_service.count(list_query).unwrap_or(0);

            // Convert to RunUsageResponse for JSON serialization
            let runs_response: Vec<RunUsageResponse> =
                runs.into_iter().map(|run| run.into()).collect();

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
