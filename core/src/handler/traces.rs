use crate::metadata::{DatabaseService, DatabaseServiceTrait};
use crate::telemetry::RunSpanBuffer;
use crate::types::handlers::pagination::PaginatedResult;
use crate::types::handlers::pagination::Pagination;
use crate::types::metadata::project::Project;
use crate::types::metadata::services::trace::{ListTracesQuery, TraceService};
use crate::types::traces::{LangdbSpan, Operation};
use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;

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

pub async fn list_traces<T: TraceService + DatabaseServiceTrait>(
    query: web::Query<ListTracesQueryParams>,
    project: web::ReqData<Project>,
    database_service: web::Data<DatabaseService>,
) -> Result<HttpResponse> {
    let trace_service = database_service.init::<T>();

    // Extract project_id from extensions (set by ProjectMiddleware)
    let project_slug = project.slug.clone();

    let thread_ids = query
        .thread_ids
        .as_ref()
        .map(|s| s.split(',').map(String::from).collect());

    let run_ids = query.run_id.as_ref().map(|id| vec![id.clone()]);

    let list_query = ListTracesQuery {
        project_slug: Some(project_slug.clone()),
        span_id: None,
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
        text_search: None,
        sort_by: None,
        sort_order: None,
    };

    Ok(trace_service
        .list_paginated(list_query.clone())
        .map(|result| HttpResponse::Ok().json(result))?)
}

#[derive(Deserialize)]
pub struct GetSpansByRunQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn get_spans_by_run<T: TraceService + DatabaseServiceTrait>(
    run_id: web::Path<String>,
    query: web::Query<GetSpansByRunQuery>,
    project: web::ReqData<Project>,
    database_service: web::Data<DatabaseService>,
    run_span_buffer: web::Data<RunSpanBuffer>,
) -> Result<HttpResponse> {
    let trace_service = database_service.init::<T>();

    let project_slug = project.slug.clone();

    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let run_span_buffer = run_span_buffer.into_inner();

    Ok(trace_service
        .get_by_run_id(
            &run_id,
            Some(&project_slug),
            limit,
            offset,
            run_span_buffer.clone(),
        )
        .map(|traces| {
            // Get child attributes for all traces
            let trace_ids: Vec<String> = traces.iter().map(|t| t.trace_id.clone()).collect();
            let span_ids: Vec<String> = traces.iter().map(|t| t.span_id.clone()).collect();

            let child_attrs = trace_service
                .get_child_attributes(&trace_ids, &span_ids, Some(&project_slug))
                .unwrap_or_default();

            let spans: Vec<LangdbSpan> = traces
                .into_iter()
                .map(|trace| {
                    let attribute = trace.parse_attribute().unwrap_or_default();

                    // Get child_attribute from the map
                    let child_attribute = child_attrs
                        .get(&trace.span_id)
                        .and_then(|opt| opt.as_ref())
                        .and_then(|json| serde_json::from_value(json.clone()).ok());
                    LangdbSpan {
                        trace_id: trace.trace_id,
                        span_id: trace.span_id,
                        thread_id: trace.thread_id,
                        parent_span_id: trace.parent_span_id,
                        operation_name: Operation::from(trace.operation_name.as_str()),
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
                project_slug: Some(project_slug.clone()),
                span_id: None,
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
                text_search: None,
                sort_by: None,
                sort_order: None,
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
