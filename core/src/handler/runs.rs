use crate::metadata::services::run::{ListRunsQuery, RunService, TypeFilter};
use crate::metadata::{DatabaseService, DatabaseServiceTrait};
use crate::types::metadata::project::Project;
use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ListRunsQueryParams {
    #[serde(alias = "runIds")]
    pub run_ids: Option<String>, // Comma-separated
    #[serde(alias = "threadIds")]
    pub thread_ids: Option<String>, // Comma-separated
    #[serde(alias = "traceIds")]
    pub trace_ids: Option<String>, // Comma-separated
    #[serde(alias = "modelName")]
    pub model_name: Option<String>,
    #[serde(alias = "typeFilter")]
    pub type_filter: Option<TypeFilter>,
    #[serde(alias = "startTimeMin")]
    pub start_time_min: Option<i64>,
    #[serde(alias = "startTimeMax")]
    pub start_time_max: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    #[serde(alias = "includeMcpTemplates")]
    pub include_mcp_templates: Option<bool>,
    /// Comma-separated labels to filter by (attribute.label)
    pub labels: Option<String>,
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
/// GET /runs/root - List root runs only
///
/// Root runs are identified by finding the first span that has a run_id
/// and doesn't have any parent_span_id. We then gather the full run info
/// for those run_ids.
///
/// Query parameters are the same as list_runs, but the results are filtered
/// to only include runs that have at least one root span.
///
/// This uses a single optimized SQL query that filters runs directly.
pub async fn list_root_runs<T: RunService + DatabaseServiceTrait>(
    query: web::Query<ListRunsQueryParams>,
    project: web::ReqData<Project>,
    database_service: web::Data<DatabaseService>,
) -> Result<HttpResponse> {
    let run_service: T = database_service.init();

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_slug = project.slug.clone();

    let list_query = ListRunsQuery {
        project_slug: Some(project_slug),
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
        labels: query
            .labels
            .as_ref()
            .map(|s| s.split(',').map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()),
    };
    let runs = run_service.list_root_runs(list_query.clone())?;
    let total = run_service.count_root_runs(list_query)?;

    let result = PaginatedResult {
        pagination: Pagination {
            offset: query.offset.unwrap_or(0),
            limit: query.limit.unwrap_or(100),
            total,
        },
        data: runs,
    };

    Ok(HttpResponse::Ok().json(result))
}

pub async fn run_by_id<T: RunService + DatabaseServiceTrait>(
    run_id: web::Path<String>,
    project: web::ReqData<Project>,
    database_service: web::Data<DatabaseService>,
) -> Result<HttpResponse> {
    let run_service: T = database_service.init();

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_slug = project.slug.clone();

    let list_query = ListRunsQuery {
        project_slug: Some(project_slug),
        run_ids: Some(vec![run_id.clone()]),
        thread_ids: None,
        trace_ids: None,
        model_name: None,
        type_filter: None,
        start_time_min: None,
        start_time_max: None,
        limit: 1,
        offset: 0,
        include_mcp_templates: false,
        labels: None,
    };
    let runs = run_service.list_root_runs(list_query.clone())?;

    Ok(HttpResponse::Ok().json(runs.first()))
}
