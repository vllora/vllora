use crate::metadata::error::DatabaseError;
use crate::metadata::models::run::RunUsageInformation;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeFilter {
    Model,
    Mcp,
}

#[derive(Debug, Clone)]
pub struct ListRunsQuery {
    pub project_slug: Option<String>,
    pub run_ids: Option<Vec<String>>,
    pub thread_ids: Option<Vec<String>>,
    pub trace_ids: Option<Vec<String>>,
    pub model_name: Option<String>,
    pub type_filter: Option<TypeFilter>,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub limit: i64,
    pub offset: i64,
    pub include_mcp_templates: bool,
    /// Labels to filter by (attribute.label) - only return runs that have spans with these labels
    pub labels: Option<Vec<String>>,
}

pub trait RunService {
    fn list(&self, query: ListRunsQuery) -> Result<Vec<RunUsageInformation>, DatabaseError>;
    fn count(&self, query: ListRunsQuery) -> Result<i64, DatabaseError>;
    fn list_root_runs(
        &self,
        query: ListRunsQuery,
    ) -> Result<Vec<RunUsageInformation>, DatabaseError>;
    fn count_root_runs(&self, query: ListRunsQuery) -> Result<i64, DatabaseError>;
}
