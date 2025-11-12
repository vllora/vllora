use serde::Deserialize;

use crate::metadata::error::DatabaseError;
use crate::metadata::models::trace::DbTrace;
use crate::telemetry::RunSpanBuffer;
use crate::types::handlers::pagination::PaginatedResult;
use crate::types::handlers::pagination::Pagination;
use crate::types::traces::LangdbSpan;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Enum representing the grouping key (discriminated union)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "group_by", content = "group_key")]
pub enum GroupByKey {
    #[serde(rename = "time")]
    Time {
        #[serde(alias = "timeBucket")]
        time_bucket: i64,
    },

    #[serde(rename = "thread")]
    Thread {
        #[serde(alias = "threadId")]
        thread_id: String,
    },

    #[serde(rename = "run")]
    Run {
        #[serde(alias = "runId")]
        run_id: uuid::Uuid,
    },
}

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

/// Query parameters for unified GET /group/spans endpoint
#[derive(Deserialize)]
pub struct GetGroupSpansQuery {
    #[serde(flatten)]
    pub group_by: GroupByKey,

    // Common parameters
    #[serde(alias = "bucketSize")]
    pub bucket_size: Option<i64>, // Only used for time grouping
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Group identifier for batch requests - discriminated union
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged, rename_all = "snake_case")]
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
        run_id: uuid::Uuid,
    },
}

impl GroupIdentifier {
    /// Generate unique key string for this group
    pub fn to_key(&self) -> String {
        match self {
            GroupIdentifier::Time { time_bucket, .. } => format!("time-{}", time_bucket),
            GroupIdentifier::Thread { thread_id } => format!("thread-{}", thread_id),
            GroupIdentifier::Run { run_id } => format!("run-{}", run_id),
        }
    }
}

fn default_spans_per_group() -> i64 {
    100
}

#[derive(Deserialize)]
pub struct BatchGroupSpansQuery {
    pub groups: Vec<GroupIdentifier>,
    #[serde(alias = "spansPerGroup", default = "default_spans_per_group")]
    pub spans_per_group: i64,
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
        run_span_buffer: Arc<RunSpanBuffer>,
    ) -> Result<Vec<DbTrace>, DatabaseError>;
    fn count(&self, query: ListTracesQuery) -> Result<i64, DatabaseError>;
    fn get_child_attributes(
        &self,
        trace_ids: &[String],
        span_ids: &[String],
        project_id: Option<&str>,
    ) -> Result<HashMap<String, Option<serde_json::Value>>, DatabaseError>;
    fn get_group_spans(
        &self,
        project_slug: &str,
        query: GetGroupSpansQuery,
    ) -> Result<PaginatedResult<LangdbSpan>, DatabaseError>;
    fn get_batch_group_spans(
        &self,
        project_slug: &str,
        query: BatchGroupSpansQuery,
    ) -> Result<BatchGroupSpansResponse, DatabaseError>;
}
