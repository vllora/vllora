use crate::metadata::DatabaseService;
use crate::metadata::DatabaseServiceTrait;
use crate::types::metadata::project::Project;
use crate::types::metadata::services::trace::{ListTracesQuery, TraceService};
use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct ListSpansQueryParams {
    #[serde(alias = "threadIds")]
    pub thread_ids: Option<String>, // Comma-separated
    #[serde(alias = "runIds")]
    pub run_ids: Option<String>, // Comma-separated
    #[serde(alias = "operationNames")]
    pub operation_names: Option<String>, // Comma-separated
    #[serde(alias = "parentSpanIds")]
    pub parent_span_ids: Option<String>, // Comma-separated
    #[serde(alias = "startTime")]
    pub start_time: Option<i64>,
    #[serde(alias = "endTime")]
    pub end_time: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct Span {
    pub trace_id: String,
    pub span_id: String,
    pub thread_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub operation_name: String,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub attribute: HashMap<String, Value>,
    pub child_attribute: Option<HashMap<String, Value>>,
    pub run_id: Option<String>,
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

/// GET /spans - List spans with optional filters
///
/// Query parameters:
/// - threadIds (optional): Filter by thread IDs (comma-separated). Special: "null"=no thread, "!null"=has thread
/// - runIds (optional): Filter by run IDs (comma-separated). Special: "null"=no run, "!null"=has run
/// - operationNames (optional): Filter by operation names (comma-separated). Special: "null"=no op, "!null"=has op
/// - parentSpanIds (optional): Filter by parent span IDs (comma-separated). Special: "null"=root, "!null"=child
/// - startTime (optional): Filter spans that started after this timestamp (microseconds)
/// - endTime (optional): Filter spans that started before this timestamp (microseconds)
/// - limit (optional): Number of results to return (default: 100)
/// - offset (optional): Number of results to skip (default: 0)
///
/// Special values (ALL filters support these):
/// - "null": Returns only spans where field IS NULL
/// - "!null": Returns only spans where field IS NOT NULL
///
/// Returns paginated list of spans with their attributes
pub async fn list_spans<T: TraceService + DatabaseServiceTrait>(
    query: web::Query<ListSpansQueryParams>,
    project: web::ReqData<Project>,
    database_service: web::Data<DatabaseService>,
) -> Result<HttpResponse> {
    let trace_service = database_service.init::<T>();

    let project_slug = project.slug.clone();

    // Parse comma-separated values into vectors
    // Handle special cases: "null" = IS NULL, "!null" = IS NOT NULL

    // Thread IDs filter
    let filter_null_thread = query
        .thread_ids
        .as_ref()
        .map(|v| v.trim().to_lowercase() == "null")
        .unwrap_or(false);
    let filter_not_null_thread = query
        .thread_ids
        .as_ref()
        .map(|v| v.trim() == "!null")
        .unwrap_or(false);
    let thread_ids = if filter_null_thread || filter_not_null_thread {
        None
    } else {
        query
            .thread_ids
            .as_ref()
            .map(|s| s.split(',').map(|id| id.trim().to_string()).collect())
    };

    // Run IDs filter
    let filter_null_run = query
        .run_ids
        .as_ref()
        .map(|v| v.trim().to_lowercase() == "null")
        .unwrap_or(false);
    let filter_not_null_run = query
        .run_ids
        .as_ref()
        .map(|v| v.trim() == "!null")
        .unwrap_or(false);
    let run_ids = if filter_null_run || filter_not_null_run {
        None
    } else {
        query
            .run_ids
            .as_ref()
            .map(|s| s.split(',').map(|id| id.trim().to_string()).collect())
    };

    // Operation names filter
    let filter_null_operation = query
        .operation_names
        .as_ref()
        .map(|v| v.trim().to_lowercase() == "null")
        .unwrap_or(false);
    let filter_not_null_operation = query
        .operation_names
        .as_ref()
        .map(|v| v.trim() == "!null")
        .unwrap_or(false);
    let operation_names = if filter_null_operation || filter_not_null_operation {
        None
    } else {
        query
            .operation_names
            .as_ref()
            .map(|s| s.split(',').map(|name| name.trim().to_string()).collect())
    };

    // Parent span IDs filter
    let filter_null_parent = query
        .parent_span_ids
        .as_ref()
        .map(|v| v.trim().to_lowercase() == "null")
        .unwrap_or(false);
    let filter_not_null_parent = query
        .parent_span_ids
        .as_ref()
        .map(|v| v.trim() == "!null")
        .unwrap_or(false);
    let parent_span_ids = if filter_null_parent || filter_not_null_parent {
        None
    } else {
        query
            .parent_span_ids
            .as_ref()
            .map(|s| s.split(',').map(|id| id.trim().to_string()).collect())
    };

    let list_query = ListTracesQuery {
        project_slug: Some(project_slug.clone()),
        run_ids,
        thread_ids,
        operation_names,
        parent_span_ids,
        // Null filters
        filter_null_thread,
        filter_null_run,
        filter_null_operation,
        filter_null_parent,
        // Not-null filters
        filter_not_null_thread,
        filter_not_null_run,
        filter_not_null_operation,
        filter_not_null_parent,
        start_time_min: query.start_time,
        start_time_max: query.end_time,
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
    };

    Ok(trace_service.list(list_query.clone()).map(|traces| {
        // Get child attributes for all traces
        let trace_ids: Vec<String> = traces.iter().map(|t| t.trace_id.clone()).collect();
        let span_ids: Vec<String> = traces.iter().map(|t| t.span_id.clone()).collect();

        let child_attrs = trace_service
            .get_child_attributes(&trace_ids, &span_ids, Some(&project_slug))
            .unwrap_or_default();

        let spans: Vec<Span> = traces
            .into_iter()
            .map(|trace| {
                let attribute = trace.parse_attribute().unwrap_or_default();

                // Get child_attribute from the map
                let child_attribute = child_attrs
                    .get(&trace.span_id)
                    .cloned()
                    .unwrap_or(None)
                    .and_then(|json| serde_json::from_value(json).ok());

                Span {
                    trace_id: trace.trace_id,
                    span_id: trace.span_id,
                    thread_id: trace.thread_id,
                    parent_span_id: trace.parent_span_id,
                    operation_name: trace.operation_name,
                    start_time_us: trace.start_time_us,
                    finish_time_us: trace.finish_time_us,
                    attribute,
                    child_attribute,
                    run_id: trace.run_id,
                }
            })
            .collect();

        let total = trace_service.count(list_query).unwrap_or(0);

        let result = PaginatedResult {
            pagination: Pagination {
                offset: query.offset.unwrap_or(0),
                limit: query.limit.unwrap_or(100),
                total,
            },
            data: spans,
        };

        HttpResponse::Ok().json(result)
    })?)
}
