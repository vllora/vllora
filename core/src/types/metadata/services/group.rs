use diesel::QueryableByName;
use serde::{Deserialize, Serialize};

use crate::metadata::error::DatabaseError;

use crate::metadata::types::JsonVec;
use diesel::sql_types::Float;
#[cfg(feature = "postgres")]
use diesel::sql_types::Jsonb;
#[cfg(feature = "sqlite")]
use diesel::sql_types::Text;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeFilter {
    Model,
    Mcp,
}

/// Enum representing the grouping type
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub enum GroupBy {
    #[default]
    Time,
    Thread,
    Run,
}

#[derive(Debug, Clone)]
pub struct ListGroupQuery {
    pub project_slug: Option<String>,
    pub thread_ids: Option<Vec<String>>,
    pub trace_ids: Option<Vec<String>>,
    pub model_name: Option<String>,
    pub type_filter: Option<TypeFilter>,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub bucket_size_seconds: i64, // Time bucket size in seconds (used when group_by=Time)
    pub group_by: GroupBy,        // NEW: Determines grouping type
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize, Deserialize, QueryableByName)]
pub struct GroupUsageInformation {
    // Grouping key fields - one will be populated depending on group_by
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>)]
    pub time_bucket: Option<i64>, // Populated when group_by=time
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub thread_id: Option<String>, // Populated when group_by=thread
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub run_id: Option<String>, // Populated when group_by=run

    // Aggregated data (same for all grouping types)
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    pub thread_ids: JsonVec,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    pub trace_ids: JsonVec,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    pub run_ids: JsonVec,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    pub root_span_ids: JsonVec,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    pub request_models: JsonVec,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    pub used_models: JsonVec,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub llm_calls: i64,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Float))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Float))]
    pub cost: f32,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>)]
    pub input_tokens: Option<i64>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>)]
    pub output_tokens: Option<i64>,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub start_time_us: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub finish_time_us: i64,
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Text))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Jsonb))]
    pub errors: JsonVec,
}

pub trait GroupService {
    fn list_root_group(
        &self,
        query: ListGroupQuery,
    ) -> Result<Vec<GroupUsageInformation>, DatabaseError>;
    fn count_root_group(&self, query: ListGroupQuery) -> Result<i64, DatabaseError>;
    fn get_by_time_bucket(
        &self,
        time_bucket: i64,
        bucket_size_seconds: i64,
        project_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::metadata::models::trace::DbTrace>, DatabaseError>;
    fn count_by_time_bucket(
        &self,
        time_bucket: i64,
        bucket_size_seconds: i64,
        project_id: Option<&str>,
    ) -> Result<i64, DatabaseError>;
}
