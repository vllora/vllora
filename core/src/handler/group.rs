use crate::metadata::models::trace::DbTrace;
use crate::metadata::pool::DbPool;
use crate::metadata::schema::traces;
use crate::metadata::DatabaseService;
use crate::metadata::DatabaseServiceTrait;
use crate::types::metadata::project::Project;
use crate::types::metadata::services::group::GroupBy;
use crate::types::metadata::services::group::GroupService;
use crate::types::metadata::services::group::GroupUsageInformation;
use crate::types::metadata::services::group::ListGroupQuery;
use crate::types::metadata::services::group::TypeFilter;
use crate::types::metadata::services::trace::GetGroupSpansQuery;
use crate::types::metadata::services::trace::GroupByKey;
use crate::types::metadata::services::trace::TraceService;
use actix_web::{web, HttpResponse, Result};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

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
    #[serde(alias = "groupBy")]
    pub group_by: Option<GroupBy>, // Grouping mode: "time" or "thread" (default: "time")
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

/// Generic response struct for all grouping types
#[derive(Debug, Serialize)]
pub struct GenericGroupResponse {
    pub group_by: GroupBy,
    #[serde(flatten)]
    pub key: GroupByKey, // Flattens the enum fields into the response
    #[serde(flatten)]
    pub group: GroupUsageInformation,
}

impl From<GroupUsageInformation> for GenericGroupResponse {
    fn from(group: GroupUsageInformation) -> Self {
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
            panic!("GroupUsageInformation must have either time_bucket, thread_id, or run_id set")
        };

        Self {
            key,
            group,
            group_by,
        }
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
    };

    let groups = group_service.list_root_group(list_query.clone())?;
    let total = group_service.count_root_group(list_query)?;

    // Transform GroupUsageInformation into GenericGroupResponse with properly typed arrays
    let group_responses: Vec<GenericGroupResponse> = groups.into_iter().map(|g| g.into()).collect();

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

/// Group identifier for batch requests - discriminated union
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "groupBy")]
pub enum GroupIdentifier {
    #[serde(rename = "time")]
    Time {
        #[serde(alias = "timeBucket")]
        time_bucket: i64,
        #[serde(alias = "bucketSize")]
        bucket_size: i64,
    },
    #[serde(rename = "thread")]
    Thread {
        #[serde(alias = "threadId")]
        thread_id: String,
    },
    #[serde(rename = "run")]
    Run {
        #[serde(alias = "runId")]
        run_id: String,
    },
}

impl GroupIdentifier {
    /// Generate unique key string for this group
    fn to_key(&self) -> String {
        match self {
            GroupIdentifier::Time { time_bucket, .. } => format!("time-{}", time_bucket),
            GroupIdentifier::Thread { thread_id } => format!("thread-{}", thread_id),
            GroupIdentifier::Run { run_id } => format!("run-{}", run_id),
        }
    }
}

/// Request body for POST /group/batch-spans endpoint
#[derive(Deserialize)]
pub struct BatchGroupSpansRequest {
    pub groups: Vec<GroupIdentifier>,
    #[serde(alias = "spansPerGroup", default = "default_spans_per_group")]
    pub spans_per_group: i64,
}

fn default_spans_per_group() -> i64 {
    100
}

/// Individual group's spans with pagination info
#[derive(Serialize)]
pub struct GroupSpansData {
    pub spans: Vec<LangdbSpan>,
    pub pagination: Pagination,
}

/// Response for batch group spans request
/// Map of groupKey -> { spans, pagination }
#[derive(Serialize)]
pub struct BatchGroupSpansResponse {
    pub data: HashMap<String, GroupSpansData>,
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
    body: web::Json<BatchGroupSpansRequest>,
    project: web::ReqData<Project>,
    database_service: web::Data<DatabaseService>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let trace_service = database_service.init::<T>();
    let project_slug = project.slug.clone();

    let limit = body.spans_per_group;
    let offset = 0; // Batch endpoint always starts from offset 0

    let mut result_map: HashMap<String, GroupSpansData> = HashMap::new();

    // Process each group sequentially to avoid SQLite lock contention
    // (even with async, SQLite serializes writes anyway)
    for group_identifier in &body.groups {
        let group_key = group_identifier.to_key();

        // Build query based on group type
        let mut conn = db_pool.get().map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Database connection error: {}", e))
        })?;

        let mut count_query = traces::table.into_boxed();
        let mut data_query = traces::table.into_boxed();

        match group_identifier {
            GroupIdentifier::Time {
                time_bucket,
                bucket_size,
            } => {
                let bucket_size_us = bucket_size * 1_000_000;
                let bucket_start = *time_bucket;
                let bucket_end = time_bucket + bucket_size_us;

                count_query = count_query
                    .filter(traces::start_time_us.ge(bucket_start))
                    .filter(traces::start_time_us.lt(bucket_end))
                    .filter(
                        traces::run_id
                            .is_not_null()
                            .or(traces::thread_id.is_not_null()),
                    );
                data_query = data_query
                    .filter(traces::start_time_us.ge(bucket_start))
                    .filter(traces::start_time_us.lt(bucket_end))
                    .filter(
                        traces::run_id
                            .is_not_null()
                            .or(traces::thread_id.is_not_null()),
                    );
            }
            GroupIdentifier::Thread { thread_id } => {
                count_query = count_query.filter(traces::thread_id.eq(thread_id.as_str()));
                data_query = data_query.filter(traces::thread_id.eq(thread_id.as_str()));
            }
            GroupIdentifier::Run { run_id } => {
                count_query = count_query.filter(traces::run_id.eq(run_id.as_str()));
                data_query = data_query.filter(traces::run_id.eq(run_id.as_str()));
            }
        }

        count_query = count_query.filter(traces::project_id.eq(&project_slug));
        data_query = data_query.filter(traces::project_id.eq(&project_slug));

        // Get total count
        let total = count_query
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap_or(0);

        // Get paginated traces
        let traces: Vec<DbTrace> = data_query
            .order(traces::start_time_us.asc())
            .limit(limit)
            .offset(offset)
            .load::<DbTrace>(&mut conn)
            .unwrap_or_default();

        // Get child attributes
        let trace_ids: Vec<String> = traces.iter().map(|t| t.trace_id.clone()).collect();
        let span_ids: Vec<String> = traces.iter().map(|t| t.span_id.clone()).collect();

        let child_attrs = trace_service
            .get_child_attributes(&trace_ids, &span_ids, Some(&project_slug))
            .unwrap_or_default();

        let spans: Vec<LangdbSpan> = traces
            .into_iter()
            .map(|trace| {
                let attribute = trace.parse_attribute().unwrap_or_default();
                let child_attribute = child_attrs
                    .get(&trace.span_id)
                    .and_then(|opt| opt.as_ref())
                    .and_then(|json_config| serde_json::from_value(json_config.clone()).ok());

                LangdbSpan {
                    trace_id: trace.trace_id,
                    span_id: trace.span_id,
                    operation_name: trace.operation_name,
                    parent_span_id: trace.parent_span_id,
                    thread_id: trace.thread_id,
                    start_time_us: trace.start_time_us,
                    finish_time_us: trace.finish_time_us,
                    attribute,
                    child_attribute,
                    run_id: trace.run_id,
                }
            })
            .collect();

        result_map.insert(
            group_key,
            GroupSpansData {
                spans,
                pagination: Pagination {
                    offset,
                    limit,
                    total,
                },
            },
        );
    }

    Ok(HttpResponse::Ok().json(BatchGroupSpansResponse { data: result_map }))
}
