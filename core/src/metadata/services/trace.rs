use crate::metadata::error::DatabaseError;
use crate::metadata::models::trace::{DbNewTrace, DbTrace};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::traces;
use crate::metadata::DatabaseServiceTrait;
use crate::types::handlers::pagination::{PaginatedResult, Pagination};
use crate::types::metadata::services::trace::GroupByKey;
use crate::types::metadata::services::trace::{GetGroupSpansQuery, ListTracesQuery, TraceService};
use crate::types::traces::{LangdbSpan, Operation};
use diesel::prelude::*;
use diesel::sql_types::{Nullable, Text};
use std::collections::HashMap;

#[derive(Clone)]
pub struct TraceServiceImpl {
    db_pool: DbPool,
}

impl DatabaseServiceTrait for TraceServiceImpl {
    fn init(db_pool: DbPool) -> Self {
        Self { db_pool }
    }
}

impl TraceService for TraceServiceImpl {
    fn list(&self, query: ListTracesQuery) -> Result<Vec<DbTrace>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut db_query = traces::table.into_boxed();

        // Apply project_id filter
        if let Some(project_slug) = &query.project_slug {
            db_query = db_query.filter(traces::project_id.eq(project_slug.clone()));
        }

        // Apply run_ids filter
        if query.filter_null_run {
            db_query = db_query.filter(traces::run_id.is_null());
        } else if query.filter_not_null_run {
            db_query = db_query.filter(traces::run_id.is_not_null());
        } else if let Some(run_ids) = &query.run_ids {
            if !run_ids.is_empty() {
                db_query = db_query.filter(traces::run_id.eq_any(run_ids));
            }
        }

        // Apply thread_ids filter
        if query.filter_null_thread {
            db_query = db_query.filter(traces::thread_id.is_null());
        } else if query.filter_not_null_thread {
            db_query = db_query.filter(traces::thread_id.is_not_null());
        } else if let Some(thread_ids) = &query.thread_ids {
            if !thread_ids.is_empty() {
                db_query = db_query.filter(traces::thread_id.eq_any(thread_ids));
            }
        }

        // Apply operation_names filter
        if query.filter_null_operation {
            db_query = db_query.filter(traces::operation_name.is_null());
        } else if query.filter_not_null_operation {
            db_query = db_query.filter(traces::operation_name.is_not_null());
        } else if let Some(operation_names) = &query.operation_names {
            if !operation_names.is_empty() {
                db_query = db_query.filter(traces::operation_name.eq_any(operation_names));
            }
        }

        // Apply parent_span_ids filter
        if query.filter_null_parent {
            db_query = db_query.filter(traces::parent_span_id.is_null());
        } else if query.filter_not_null_parent {
            db_query = db_query.filter(traces::parent_span_id.is_not_null());
        } else if let Some(parent_span_ids) = &query.parent_span_ids {
            if !parent_span_ids.is_empty() {
                db_query = db_query.filter(traces::parent_span_id.eq_any(parent_span_ids));
            }
        }

        // Apply time range filters
        if let Some(start_min) = query.start_time_min {
            db_query = db_query.filter(traces::start_time_us.ge(start_min));
        }

        if let Some(start_max) = query.start_time_max {
            db_query = db_query.filter(traces::start_time_us.le(start_max));
        }

        // Order by start_time_us descending, apply limit and offset
        let results = db_query
            .order(traces::start_time_us.desc())
            .limit(query.limit)
            .offset(query.offset)
            .load::<DbTrace>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(results)
    }

    fn list_paginated(
        &self,
        query: ListTracesQuery,
    ) -> Result<PaginatedResult<LangdbSpan>, DatabaseError> {
        self.list(query.clone()).map(|traces| {
            let mut pagination = Pagination {
                offset: query.offset,
                limit: query.limit,
                total: 0,
            };
            if traces.is_empty() {
                return PaginatedResult::<LangdbSpan>::new(vec![], pagination);
            }

            // Get child attributes for all traces
            let trace_ids: Vec<String> = traces.iter().map(|t| t.trace_id.clone()).collect();
            let span_ids: Vec<String> = traces.iter().map(|t| t.span_id.clone()).collect();
            let project_id = traces.first().and_then(|t| t.project_id.clone());

            let child_attrs = self
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

            let total = self.count(query.clone()).unwrap_or(0);
            pagination.total = total;
            PaginatedResult::<LangdbSpan>::new(spans, pagination)
        })
    }

    fn get_by_run_id(
        &self,
        run_id: &str,
        project_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DbTrace>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut query = traces::table.filter(traces::run_id.eq(run_id)).into_boxed();

        // Apply project_id filter if provided
        if let Some(project_id) = project_id {
            query = query.filter(traces::project_id.eq(project_id));
        }

        let results = query
            .order(traces::start_time_us.asc())
            .limit(limit)
            .offset(offset)
            .load::<DbTrace>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(results)
    }

    fn count(&self, query: ListTracesQuery) -> Result<i64, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut db_query = traces::table.into_boxed();

        // Apply project_id filter
        if let Some(project_id) = &query.project_slug {
            db_query = db_query.filter(traces::project_id.eq(project_id));
        }

        // Apply run_ids filter
        if query.filter_null_run {
            db_query = db_query.filter(traces::run_id.is_null());
        } else if query.filter_not_null_run {
            db_query = db_query.filter(traces::run_id.is_not_null());
        } else if let Some(run_ids) = &query.run_ids {
            if !run_ids.is_empty() {
                db_query = db_query.filter(traces::run_id.eq_any(run_ids));
            }
        }

        // Apply thread_ids filter
        if query.filter_null_thread {
            db_query = db_query.filter(traces::thread_id.is_null());
        } else if query.filter_not_null_thread {
            db_query = db_query.filter(traces::thread_id.is_not_null());
        } else if let Some(thread_ids) = &query.thread_ids {
            if !thread_ids.is_empty() {
                db_query = db_query.filter(traces::thread_id.eq_any(thread_ids));
            }
        }

        // Apply operation_names filter
        if query.filter_null_operation {
            db_query = db_query.filter(traces::operation_name.is_null());
        } else if query.filter_not_null_operation {
            db_query = db_query.filter(traces::operation_name.is_not_null());
        } else if let Some(operation_names) = &query.operation_names {
            if !operation_names.is_empty() {
                db_query = db_query.filter(traces::operation_name.eq_any(operation_names));
            }
        }

        // Apply parent_span_ids filter
        if query.filter_null_parent {
            db_query = db_query.filter(traces::parent_span_id.is_null());
        } else if query.filter_not_null_parent {
            db_query = db_query.filter(traces::parent_span_id.is_not_null());
        } else if let Some(parent_span_ids) = &query.parent_span_ids {
            if !parent_span_ids.is_empty() {
                db_query = db_query.filter(traces::parent_span_id.eq_any(parent_span_ids));
            }
        }

        // Apply time range filters
        if let Some(start_min) = query.start_time_min {
            db_query = db_query.filter(traces::start_time_us.ge(start_min));
        }

        if let Some(start_max) = query.start_time_max {
            db_query = db_query.filter(traces::start_time_us.le(start_max));
        }

        let count = db_query
            .count()
            .get_result::<i64>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(count)
    }

    fn get_child_attributes(
        &self,
        trace_ids: &[String],
        span_ids: &[String],
        project_id: Option<&str>,
    ) -> Result<HashMap<String, Option<serde_json::Value>>, DatabaseError> {
        if trace_ids.is_empty() || span_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.db_pool.get()?;

        // Build the query to find model_call children for each parent span
        // Using a subquery to get the first model_call child for each parent
        #[derive(QueryableByName)]
        struct ChildAttr {
            #[diesel(sql_type = Text)]
            parent_span_id: String,
            #[diesel(sql_type = Nullable<Text>)]
            child_attribute: Option<String>,
        }

        let trace_ids_str = trace_ids
            .iter()
            .map(|id| format!("'{}'", id))
            .collect::<Vec<_>>()
            .join(",");

        let span_ids_str = span_ids
            .iter()
            .map(|id| format!("'{}'", id))
            .collect::<Vec<_>>()
            .join(",");

        let project_filter = if let Some(pid) = project_id {
            format!(" AND project_id = '{}'", pid)
        } else {
            String::new()
        };

        let query = format!(
            "SELECT parent_span_id, attribute as child_attribute
             FROM (
                 SELECT parent_span_id, attribute,
                        ROW_NUMBER() OVER (PARTITION BY parent_span_id ORDER BY start_time_us) as rn
                 FROM traces
                 WHERE trace_id IN ({})
                   AND parent_span_id IN ({})
                   AND operation_name = 'model_call'{}
             )
             WHERE rn = 1",
            trace_ids_str, span_ids_str, project_filter
        );

        let results = diesel::sql_query(query)
            .load::<ChildAttr>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        let mut map = HashMap::new();
        for result in results {
            map.insert(
                result.parent_span_id,
                result
                    .child_attribute
                    .map(|attr| serde_json::from_str(&attr).unwrap()),
            );
        }
        Ok(map)
    }

    fn get_group_spans(
        &self,
        project_slug: &str,
        query: GetGroupSpansQuery,
    ) -> Result<PaginatedResult<LangdbSpan>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let limit = query.limit.unwrap_or(100);
        let offset = query.offset.unwrap_or(0);

        // Build base query with appropriate filter based on group_by
        let mut count_query = traces::table.into_boxed();
        let mut data_query = traces::table.into_boxed();

        match &query.group_by {
            GroupByKey::Time { time_bucket } => {
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
            GroupByKey::Thread { thread_id } => {
                count_query = count_query.filter(traces::thread_id.eq(thread_id.as_str()));
                data_query = data_query.filter(traces::thread_id.eq(thread_id.as_str()));
            }
            GroupByKey::Run { run_id } => {
                count_query = count_query.filter(traces::run_id.eq(run_id.to_string()));
                data_query = data_query.filter(traces::run_id.eq(run_id.to_string()));
            }
        }

        // Add project_id filter if present
        count_query = count_query.filter(traces::project_id.eq(&project_slug));
        data_query = data_query.filter(traces::project_id.eq(&project_slug));

        // Get total count
        let total = count_query.count().get_result::<i64>(&mut conn)?;

        // Get paginated traces
        let traces: Vec<DbTrace> = data_query
            .order(traces::start_time_us.asc())
            .limit(limit)
            .offset(offset)
            .load::<DbTrace>(&mut conn)?;

        // Get child attributes
        let trace_ids: Vec<String> = traces.iter().map(|t| t.trace_id.clone()).collect();
        let span_ids: Vec<String> = traces.iter().map(|t| t.span_id.clone()).collect();

        let child_attrs = self
            .get_child_attributes(&trace_ids, &span_ids, Some(project_slug))
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
                    operation_name: Operation::from(trace.operation_name.as_str()),
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

        Ok(PaginatedResult {
            pagination: Pagination {
                offset,
                limit,
                total,
            },
            data: spans,
        })
    }
}

impl TraceServiceImpl {
    pub fn insert_many(&self, trace_list: Vec<DbNewTrace>) -> Result<usize, DatabaseError> {
        if trace_list.is_empty() {
            return Ok(0);
        }

        let mut conn = self.db_pool.get()?;
        let mut inserted_count = 0;

        for trace in &trace_list {
            diesel::insert_into(traces::table)
                .values(trace)
                .execute(&mut conn)?;
            inserted_count += 1;
        }

        Ok(inserted_count)
    }
}
