use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, Result};
use langdb_core::metadata::models::project::DbProject;
use langdb_core::metadata::pool::DbPool;
use langdb_core::metadata::services::group::{GroupService, GroupServiceImpl, ListGroupQuery, TypeFilter};
use langdb_core::metadata::services::trace::{ListTracesQuery, TraceService, TraceServiceImpl};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ListGroupQueryParams {
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
    #[serde(alias = "bucketSize")]
    pub bucket_size: Option<i64>, // Time bucket size in seconds
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

/// GET /group - List root spans grouped by time buckets
///
/// This endpoint groups root spans (spans with no parent_span_id) into time buckets
/// based on their start_time_us. The bucket_size parameter determines the granularity
/// of grouping (e.g., 3600 for 1 hour buckets, 7200 for 2 hour buckets).
///
/// Query parameters:
/// - bucket_size: Time bucket size in seconds (default: 3600 = 1 hour)
/// - thread_ids: Comma-separated list of thread IDs to filter by
/// - trace_ids: Comma-separated list of trace IDs to filter by
/// - model_name: Filter by model name
/// - type_filter: Filter by type (model or mcp)
/// - start_time_min: Minimum start time in microseconds
/// - start_time_max: Maximum start time in microseconds
/// - limit: Number of results to return (default: 100)
/// - offset: Number of results to skip (default: 0)
pub async fn list_root_group(
    req: HttpRequest,
    query: web::Query<ListGroupQueryParams>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let group_service: GroupServiceImpl = GroupServiceImpl::new(Arc::new(db_pool.get_ref().clone()));

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

    let list_query = ListGroupQuery {
        project_id: project_id.clone(),
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
        bucket_size_seconds: query.bucket_size.unwrap_or(3600), // Default to 1 hour
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
    };

    let groups = group_service.list_root_group(list_query.clone())?;
    let total = group_service.count_root_group(list_query)?;

    let result = PaginatedResult {
        pagination: Pagination {
            offset: query.offset.unwrap_or(0),
            limit: query.limit.unwrap_or(100),
            total,
        },
        data: groups,
    };

    Ok(HttpResponse::Ok().json(result))
}

#[derive(Serialize)]
pub struct LangdbSpan {
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

#[derive(Deserialize)]
pub struct GetSpansByGroupQuery {
    #[serde(alias = "bucketSize")]
    pub bucket_size: Option<i64>, // Time bucket size in seconds
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /group/{time_bucket} - Get all spans in a specific time bucket
///
/// This endpoint retrieves all root spans (spans with no parent_span_id) that fall
/// within the specified time bucket.
///
/// Path parameters:
/// - time_bucket: The start timestamp of the bucket in microseconds
///
/// Query parameters:
/// - bucket_size: Time bucket size in seconds (default: 3600 = 1 hour)
/// - limit: Number of results to return (default: 100)
/// - offset: Number of results to skip (default: 0)
pub async fn get_spans_by_group(
    req: HttpRequest,
    time_bucket: web::Path<i64>,
    query: web::Query<GetSpansByGroupQuery>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let group_service = GroupServiceImpl::new(Arc::new(db_pool.get_ref().clone()));
    let trace_service = TraceServiceImpl::new(Arc::new(db_pool.get_ref().clone()));

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

    let bucket_size_seconds = query.bucket_size.unwrap_or(3600);
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    Ok(group_service
        .get_by_time_bucket(
            *time_bucket,
            bucket_size_seconds,
            project_id.as_deref(),
            limit,
            offset,
        )
        .map(|traces| {
            // Get child attributes for all traces
            let trace_ids: Vec<String> = traces.iter().map(|t| t.trace_id.clone()).collect();
            let span_ids: Vec<String> = traces.iter().map(|t| t.span_id.clone()).collect();

            let child_attrs = trace_service
                .get_child_attributes(&trace_ids, &span_ids, project_id.as_deref())
                .unwrap_or_default();

            let spans: Vec<LangdbSpan> = traces
                .into_iter()
                .map(|trace| {
                    let attribute = trace.parse_attribute().unwrap_or_default();

                    // Get child_attribute from the map
                    let child_attribute = child_attrs
                        .get(&trace.span_id)
                        .and_then(|opt| opt.as_ref())
                        .and_then(|json_str| serde_json::from_str(json_str).ok());

                    LangdbSpan {
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

            // Get total count for this time bucket
            let bucket_size_us = bucket_size_seconds * 1_000_000;
            let bucket_start = *time_bucket;
            let bucket_end = bucket_start + bucket_size_us;

            let count_query = ListTracesQuery {
                project_id: project_id.clone(),
                run_ids: None,
                thread_ids: None,
                operation_names: None,
                parent_span_ids: None,
                filter_null_thread: false,
                filter_null_run: false,
                filter_null_operation: false,
                filter_null_parent: true, // Only count root spans
                filter_not_null_thread: false,
                filter_not_null_run: false,
                filter_not_null_operation: false,
                filter_not_null_parent: false,
                start_time_min: Some(bucket_start),
                start_time_max: Some(bucket_end),
                limit: 1,
                offset: 0,
            };
            let total = trace_service.count(count_query).unwrap_or(0);

            let result = PaginatedResult {
                pagination: Pagination {
                    offset,
                    limit,
                    total,
                },
                data: spans,
            };

            HttpResponse::Ok().json(result)
        })?)
}
