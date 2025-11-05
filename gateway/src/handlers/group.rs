use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, Result};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use vllora_core::metadata::models::project::DbProject;
use vllora_core::metadata::models::trace::DbTrace;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::schema::traces;
use vllora_core::metadata::services::group::{
    GroupBy, GroupService, GroupServiceImpl, GroupUsageInformation, ListGroupQuery, TypeFilter,
};
use vllora_core::metadata::services::trace::{TraceService, TraceServiceImpl};

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
    pub group_by: Option<String>, // Grouping mode: "time" or "thread" (default: "time")
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

/// Enum representing the grouping key (discriminated union)
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "group_by", content = "group_key")]
pub enum GroupByKey {
    #[serde(rename = "time")]
    Time { time_bucket: i64 },

    #[serde(rename = "thread")]
    Thread { thread_id: String },

    #[serde(rename = "run")]
    Run { run_id: String },
    // Future grouping types can be added here:
    // #[serde(rename = "model")]
    // Model { model_name: String },
}

/// Generic response struct for all grouping types
#[derive(Debug, Serialize)]
pub struct GenericGroupResponse {
    #[serde(flatten)]
    pub key: GroupByKey, // Flattens the enum fields into the response
    pub thread_ids: Vec<String>,
    pub trace_ids: Vec<String>,
    pub run_ids: Vec<String>,
    pub root_span_ids: Vec<String>,
    pub request_models: Vec<String>,
    pub used_models: Vec<String>,
    pub llm_calls: i64,
    pub cost: f64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub errors: Vec<String>,
}

impl From<GroupUsageInformation> for GenericGroupResponse {
    fn from(group: GroupUsageInformation) -> Self {
        // Parse JSON string fields into proper arrays
        let thread_ids: Vec<String> =
            serde_json::from_str(&group.thread_ids_json).unwrap_or_default();
        let trace_ids: Vec<String> =
            serde_json::from_str(&group.trace_ids_json).unwrap_or_default();
        let run_ids: Vec<String> = serde_json::from_str(&group.run_ids_json).unwrap_or_default();
        let root_span_ids: Vec<String> =
            serde_json::from_str(&group.root_span_ids_json).unwrap_or_default();
        let request_models: Vec<String> =
            serde_json::from_str(&group.request_models_json).unwrap_or_default();
        let used_models: Vec<String> =
            serde_json::from_str(&group.used_models_json).unwrap_or_default();
        let errors: Vec<String> = serde_json::from_str(&group.errors_json).unwrap_or_default();

        // Determine which grouping key to use
        let key = if let Some(time_bucket) = group.time_bucket {
            GroupByKey::Time { time_bucket }
        } else if let Some(thread_id) = group.thread_id {
            GroupByKey::Thread { thread_id }
        } else if let Some(run_id) = group.run_id {
            GroupByKey::Run { run_id }
        } else {
            // This shouldn't happen if SQL is correct
            panic!("GroupUsageInformation must have either time_bucket, thread_id, or run_id set")
        };

        Self {
            key,
            thread_ids,
            trace_ids,
            run_ids,
            root_span_ids,
            request_models,
            used_models,
            llm_calls: group.llm_calls,
            cost: group.cost,
            input_tokens: group.input_tokens,
            output_tokens: group.output_tokens,
            start_time_us: group.start_time_us,
            finish_time_us: group.finish_time_us,
            errors,
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
pub async fn list_root_group(
    req: HttpRequest,
    query: web::Query<ListGroupQueryParams>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let group_service: GroupServiceImpl = GroupServiceImpl::new(db_pool.get_ref().clone());

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

    // Parse group_by parameter (default: "time" for backward compatibility)
    let group_by = match query.group_by.as_deref().unwrap_or("time") {
        "time" => GroupBy::Time,
        "thread" => GroupBy::Thread,
        "run" => GroupBy::Run,
        other => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": format!("Invalid group_by parameter: '{}'. Must be 'time', 'thread', or 'run'", other)
            })))
        }
    };

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
        group_by,                                               // NEW: Add GroupBy enum
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

/// Query parameters for unified GET /group/spans endpoint
#[derive(Deserialize)]
pub struct GetGroupSpansQuery {
    #[serde(alias = "groupBy")]
    pub group_by: Option<String>, // 'time', 'thread', or 'run'

    // Group-specific identifiers (one should be provided based on group_by)
    #[serde(alias = "timeBucket")]
    pub time_bucket: Option<i64>, // For time grouping
    #[serde(alias = "threadId")]
    pub thread_id: Option<String>, // For thread grouping
    #[serde(alias = "runId")]
    pub run_id: Option<String>, // For run grouping

    // Common parameters
    #[serde(alias = "bucketSize")]
    pub bucket_size: Option<i64>, // Only used for time grouping
    pub limit: Option<i64>,
    pub offset: Option<i64>,
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
pub async fn get_group_spans(
    req: HttpRequest,
    query: web::Query<GetGroupSpansQuery>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let trace_service = TraceServiceImpl::new(db_pool.get_ref().clone());

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let group_by = query.group_by.as_deref().unwrap_or("time");

    // Unified query for all grouping types - just different filter conditions
    let mut conn = db_pool.get().map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Database connection error: {}", e))
    })?;

    // Build base query with appropriate filter based on group_by
    let mut count_query = traces::table.into_boxed();
    let mut data_query = traces::table.into_boxed();

    match group_by {
        "time" => {
            let time_bucket = query.time_bucket.ok_or_else(|| {
                actix_web::error::ErrorBadRequest("timeBucket is required for groupBy=time")
            })?;
            let bucket_size_seconds = query.bucket_size.unwrap_or(3600);
            let bucket_size_us = bucket_size_seconds * 1_000_000;
            let bucket_start = time_bucket;
            let bucket_end = time_bucket + bucket_size_us;

            // Filter by time range and ensure run_id or thread_id is not null
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
        "thread" => {
            let thread_id = query.thread_id.as_ref().ok_or_else(|| {
                actix_web::error::ErrorBadRequest("threadId is required for groupBy=thread")
            })?;
            count_query = count_query.filter(traces::thread_id.eq(thread_id.as_str()));
            data_query = data_query.filter(traces::thread_id.eq(thread_id.as_str()));
        }
        "run" => {
            let run_id = query.run_id.as_ref().ok_or_else(|| {
                actix_web::error::ErrorBadRequest("runId is required for groupBy=run")
            })?;
            count_query = count_query.filter(traces::run_id.eq(run_id.as_str()));
            data_query = data_query.filter(traces::run_id.eq(run_id.as_str()));
        }
        other => {
            return Err(actix_web::error::ErrorBadRequest(format!(
                "Invalid groupBy parameter: '{}'. Must be 'time', 'thread', or 'run'",
                other
            )));
        }
    }

    // Add project_id filter if present
    if let Some(ref proj_id) = project_id {
        count_query = count_query.filter(traces::project_id.eq(proj_id));
        data_query = data_query.filter(traces::project_id.eq(proj_id));
    }

    // Get total count
    let total = count_query
        .count()
        .get_result::<i64>(&mut conn)
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Database query error: {}", e))
        })?;

    // Get paginated traces
    let traces: Vec<DbTrace> = data_query
        .order(traces::start_time_us.asc())
        .limit(limit)
        .offset(offset)
        .load::<DbTrace>(&mut conn)
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("Database query error: {}", e))
        })?;

    // Get child attributes
    let trace_ids: Vec<String> = traces.iter().map(|t| t.trace_id.clone()).collect();
    let span_ids: Vec<String> = traces.iter().map(|t| t.span_id.clone()).collect();

    let child_attrs = trace_service
        .get_child_attributes(&trace_ids, &span_ids, project_id.as_deref())
        .unwrap_or_default();

    let spans: Vec<LangdbSpan> = traces
        .into_iter()
        .map(|trace| {
            let attribute = trace.parse_attribute().unwrap_or_default();
            let child_attribute = child_attrs
                .get(&trace.span_id)
                .and_then(|opt| opt.as_ref())
                .and_then(|json_str| serde_json::from_str(json_str).ok());

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

    Ok(HttpResponse::Ok().json(PaginatedResult {
        pagination: Pagination {
            offset,
            limit,
            total,
        },
        data: spans,
    }))
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
pub async fn get_batch_group_spans(
    req: HttpRequest,
    body: web::Json<BatchGroupSpansRequest>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let trace_service = TraceServiceImpl::new(db_pool.get_ref().clone());
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

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

        // Add project_id filter if present
        if let Some(ref proj_id) = project_id {
            count_query = count_query.filter(traces::project_id.eq(proj_id));
            data_query = data_query.filter(traces::project_id.eq(proj_id));
        }

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
            .get_child_attributes(&trace_ids, &span_ids, project_id.as_deref())
            .unwrap_or_default();

        let spans: Vec<LangdbSpan> = traces
            .into_iter()
            .map(|trace| {
                let attribute = trace.parse_attribute().unwrap_or_default();
                let child_attribute = child_attrs
                    .get(&trace.span_id)
                    .and_then(|opt| opt.as_ref())
                    .and_then(|json_str| serde_json::from_str(json_str).ok());

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
