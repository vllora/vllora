use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use vllora_core::metadata::models::project::DbProject;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::trace::{ListTracesQuery, TraceService, TraceServiceImpl};

#[derive(Deserialize)]
pub struct ListTracesQueryParams {
    #[serde(alias = "runId")]
    run_id: Option<String>,
    #[serde(alias = "threadIds")]
    thread_ids: Option<String>, // comma-separated
    #[serde(alias = "startTimeMin")]
    start_time_min: Option<i64>,
    #[serde(alias = "startTimeMax")]
    start_time_max: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
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

pub async fn list_traces(
    req: HttpRequest,
    query: web::Query<ListTracesQueryParams>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let trace_service = TraceServiceImpl::new(Arc::new(db_pool.get_ref().clone()));

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

    let thread_ids = query
        .thread_ids
        .as_ref()
        .map(|s| s.split(',').map(String::from).collect());

    let run_ids = query.run_id.as_ref().map(|id| vec![id.clone()]);

    let list_query = ListTracesQuery {
        project_id: project_id.clone(),
        run_ids,
        thread_ids,
        operation_names: None,
        parent_span_ids: None,
        filter_null_thread: false,
        filter_null_run: false,
        filter_null_operation: false,
        filter_null_parent: false,
        filter_not_null_thread: false,
        filter_not_null_run: false,
        filter_not_null_operation: false,
        filter_not_null_parent: false,
        start_time_min: query.start_time_min,
        start_time_max: query.start_time_max,
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
    };

    Ok(trace_service.list(list_query.clone()).map(|traces| {
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

#[derive(Deserialize)]
pub struct GetSpansByRunQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn get_spans_by_run(
    req: HttpRequest,
    run_id: web::Path<String>,
    query: web::Query<GetSpansByRunQuery>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse> {
    let trace_service = TraceServiceImpl::new(Arc::new(db_pool.get_ref().clone()));

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_id = req.extensions().get::<DbProject>().map(|p| p.slug.clone());

    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    Ok(trace_service
        .get_by_run_id(&run_id, project_id.as_deref(), limit, offset)
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

            // Get total count for this run_id
            let count_query = ListTracesQuery {
                project_id: project_id.clone(),
                run_ids: Some(vec![run_id.into_inner()]),
                thread_ids: None,
                operation_names: None,
                parent_span_ids: None,
                filter_null_thread: false,
                filter_null_run: false,
                filter_null_operation: false,
                filter_null_parent: false,
                filter_not_null_thread: false,
                filter_not_null_run: false,
                filter_not_null_operation: false,
                filter_not_null_parent: false,
                start_time_min: None,
                start_time_max: None,
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
