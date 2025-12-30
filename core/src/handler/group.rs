use crate::error::GatewayError;
use crate::metadata::DatabaseService;
use crate::metadata::DatabaseServiceTrait;
use crate::types::handlers::pagination::PaginatedResult;
use crate::types::handlers::pagination::Pagination;
use crate::types::metadata::project::Project;
use crate::types::metadata::services::group::GroupBy;
use crate::types::metadata::services::group::GroupService;
use crate::types::metadata::services::group::GroupUsageInformation;
use crate::types::metadata::services::group::ListGroupQuery;
use crate::types::metadata::services::group::TypeFilter;
use crate::types::metadata::services::trace::BatchGroupSpansQuery;
use crate::types::metadata::services::trace::GetGroupSpansQuery;
use crate::types::metadata::services::trace::GroupByKey;
use crate::types::metadata::services::trace::TraceService;
use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};

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
    #[serde(alias = "startTimeMin")]
    pub start_time_min: Option<i64>,
    #[serde(alias = "startTimeMax")]
    pub start_time_max: Option<i64>,
    #[serde(alias = "bucketSize")]
    pub bucket_size: Option<i64>, // Time bucket size in seconds
    #[serde(alias = "groupBy")]
    pub group_by: Option<GroupBy>, // Grouping mode: "time" or "thread" (default: "time")
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// Comma-separated labels to filter by (attribute.label)
    pub labels: Option<String>,
}

/// Generic response struct for all grouping types
#[derive(Debug, Serialize)]
pub struct GenericGroupResponse {
    pub group_by: GroupBy,
    #[serde(flatten)]
    pub key: GroupByKey, // Flattens the enum fields into the response
    #[serde(flatten)]
    pub group: GroupUsageInformation,
}

impl TryFrom<GroupUsageInformation> for GenericGroupResponse {
    type Error = GatewayError;

    fn try_from(group: GroupUsageInformation) -> Result<Self, Self::Error> {
        // Determine which grouping key to use
        let (key, group_by) = if let Some(time_bucket) = &group.time_bucket {
            (
                GroupByKey::Time {
                    time_bucket: *time_bucket,
                },
                GroupBy::Time,
            )
        } else if let Some(thread_id) = &group.thread_id {
            (
                GroupByKey::Thread {
                    thread_id: thread_id.clone(),
                },
                GroupBy::Thread,
            )
        } else if let Some(run_id) = &group.run_id {
            (
                GroupByKey::Run {
                    run_id: (*run_id).into(),
                },
                GroupBy::Run,
            )
        } else {
            // This shouldn't happen if SQL is correct
            return Err(GatewayError::CustomError(
                "GroupUsageInformation must have either time_bucket, thread_id, or run_id set"
                    .to_string(),
            ));
        };

        Ok(GenericGroupResponse {
            key,
            group,
            group_by,
        })
    }
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
pub async fn list_root_group<T: GroupService + DatabaseServiceTrait>(
    query: web::Query<ListGroupQueryParams>,
    database_service: web::Data<DatabaseService>,
    project: web::ReqData<Project>,
) -> Result<HttpResponse> {
    let group_service = database_service.init::<T>();

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_slug = project.slug.clone();

    let list_query = ListGroupQuery {
        project_slug: Some(project_slug),
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
        group_by: query.group_by.clone().unwrap_or_default(),
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
        labels: query
            .labels
            .as_ref()
            .map(|s| s.split(',').map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()),
    };

    let groups = group_service.list_root_group(list_query.clone())?;
    let total = group_service.count_root_group(list_query)?;

    // Transform GroupUsageInformation into GenericGroupResponse with properly typed arrays
    let group_responses = groups
        .into_iter()
        .map(|g| g.try_into())
        .collect::<Result<Vec<GenericGroupResponse>, GatewayError>>()?;

    let result = PaginatedResult {
        pagination: Pagination {
            offset: query.offset.unwrap_or(0),
            limit: query.limit.unwrap_or(100),
            total,
        },
        data: group_responses,
    };

    Ok(HttpResponse::Ok().json(result))
}

/// GET /group/spans - Unified endpoint to get spans for any group type
///
/// This endpoint retrieves spans based on the grouping type specified in query parameters.
///
/// Query parameters:
/// - groupBy: 'time', 'thread', or 'run' (default: 'time')
/// - timeBucket: timestamp in microseconds (required if groupBy='time')
/// - threadId: thread identifier (required if groupBy='thread')
/// - runId: run identifier (required if groupBy='run')
/// - bucketSize: time bucket size in seconds (only for groupBy='time', default: 3600)
/// - limit: number of results to return (default: 100)
/// - offset: number of results to skip (default: 0)
pub async fn get_group_spans<T: TraceService + DatabaseServiceTrait>(
    query: web::Query<GetGroupSpansQuery>,
    project: web::ReqData<Project>,
    database_service: web::Data<DatabaseService>,
) -> Result<HttpResponse> {
    let trace_service = database_service.init::<T>();

    let project_slug = project.slug.clone();

    let result = trace_service.get_group_spans(&project_slug, query.into_inner())?;

    Ok(HttpResponse::Ok().json(result))
}

/// POST /group/batch-spans - Get spans for multiple groups in a single request
///
/// This endpoint allows batching multiple group span requests into one, reducing
/// HTTP overhead and database connection usage. Particularly useful for SQLite where
/// concurrent requests queue at the DB level anyway.
///
/// Request body:
/// ```json
/// {
///   "groups": [
///     { "groupBy": "time", "timeBucket": 1234567890, "bucketSize": 300 },
///     { "groupBy": "thread", "threadId": "thread-123" },
///     { "groupBy": "run", "runId": "run-456" }
///   ],
///   "spansPerGroup": 100
/// }
/// ```
///
/// Response:
/// ```json
/// {
///   "data": {
///     "time-1234567890": {
///       "spans": [...],
///       "pagination": { "total": 450, "offset": 0, "limit": 100 }
///     },
///     "thread-thread-123": {
///       "spans": [...],
///       "pagination": { "total": 12, "offset": 0, "limit": 100 }
///     }
///   }
/// }
/// ```
pub async fn get_batch_group_spans<T: TraceService + DatabaseServiceTrait>(
    body: web::Json<BatchGroupSpansQuery>,
    project: web::ReqData<Project>,
    database_service: web::Data<DatabaseService>,
) -> Result<HttpResponse> {
    let trace_service = database_service.init::<T>();
    let project_slug = project.slug.clone();

    let result = trace_service.get_batch_group_spans(&project_slug, body.into_inner())?;

    Ok(HttpResponse::Ok().json(result))
}
