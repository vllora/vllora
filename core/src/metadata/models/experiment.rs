use crate::metadata::schema::experiments;
use diesel::helper_types::AsSelect;
use diesel::helper_types::Select;
#[cfg(feature = "postgres")]
use diesel::pg::Pg;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
use diesel::{AsChangeset, Insertable, Queryable, QueryableByName, Selectable};
use diesel::{Identifiable, SelectableHelper};
use serde::{Deserialize, Serialize};

#[derive(
    QueryableByName,
    Selectable,
    Queryable,
    PartialEq,
    Eq,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Identifiable,
    AsChangeset,
)]
#[serde(crate = "serde")]
#[diesel(table_name = experiments)]
pub struct DbExperiment {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub original_span_id: String,
    pub original_trace_id: String,
    pub original_request: String, // JSON as string
    pub modified_request: String, // JSON as string
    pub headers: Option<String>, // JSON as string
    pub prompt_variables: Option<String>, // JSON as string
    pub model_parameters: Option<String>, // JSON as string
    pub result_span_id: Option<String>,
    pub result_trace_id: Option<String>,
    pub status: String, // draft, running, completed, failed
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(feature = "sqlite")]
type All = Select<experiments::table, AsSelect<DbExperiment, Sqlite>>;
#[cfg(feature = "postgres")]
type All = Select<experiments::table, AsSelect<DbExperiment, Pg>>;

impl DbExperiment {
    pub fn all() -> All {
        diesel::QueryDsl::select(experiments::table, DbExperiment::as_select())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Insertable)]
#[serde(crate = "serde")]
#[diesel(table_name = experiments)]
pub struct NewDbExperiment {
    pub name: String,
    pub description: Option<String>,
    pub original_span_id: String,
    pub original_trace_id: String,
    pub original_request: String,
    pub modified_request: String,
    pub headers: Option<String>,
    pub prompt_variables: Option<String>,
    pub model_parameters: Option<String>,
    pub status: String,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, AsChangeset)]
#[serde(crate = "serde")]
#[diesel(table_name = experiments)]
pub struct UpdateDbExperiment {
    pub name: Option<String>,
    pub description: Option<String>,
    pub modified_request: Option<String>,
    pub headers: Option<String>,
    pub prompt_variables: Option<String>,
    pub model_parameters: Option<String>,
    pub result_span_id: Option<String>,
    pub result_trace_id: Option<String>,
    pub status: Option<String>,
    pub updated_at: String,
}
