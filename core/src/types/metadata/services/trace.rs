use crate::metadata::error::DatabaseError;
use crate::metadata::models::trace::DbTrace;
use crate::types::handlers::pagination::PaginatedResult;
use crate::types::traces::LangdbSpan;
use std::collections::HashMap;

#[derive(Debug, Clone)]

pub struct ListTracesQuery {
    pub project_slug: Option<String>,
    pub run_ids: Option<Vec<String>>,
    pub thread_ids: Option<Vec<String>>,
    pub operation_names: Option<Vec<String>>,
    pub parent_span_ids: Option<Vec<String>>,
    // Null filters (IS NULL)
    pub filter_null_thread: bool,
    pub filter_null_run: bool,
    pub filter_null_operation: bool,
    pub filter_null_parent: bool,
    // Not-null filters (IS NOT NULL)
    pub filter_not_null_thread: bool,
    pub filter_not_null_run: bool,
    pub filter_not_null_operation: bool,
    pub filter_not_null_parent: bool,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

impl Default for ListTracesQuery {
    fn default() -> Self {
        Self {
            project_slug: None,
            run_ids: None,
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
            limit: 100,
            offset: 0,
        }
    }
}

pub trait TraceService {
    fn list(&self, query: ListTracesQuery) -> Result<Vec<DbTrace>, DatabaseError>;
    fn list_paginated(
        &self,
        query: ListTracesQuery,
    ) -> Result<PaginatedResult<LangdbSpan>, DatabaseError>;
    fn get_by_run_id(
        &self,
        run_id: &str,
        project_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DbTrace>, DatabaseError>;
    fn count(&self, query: ListTracesQuery) -> Result<i64, DatabaseError>;
    fn get_child_attributes(
        &self,
        trace_ids: &[String],
        span_ids: &[String],
        project_id: Option<&str>,
    ) -> Result<HashMap<String, Option<serde_json::Value>>, DatabaseError>;
}
