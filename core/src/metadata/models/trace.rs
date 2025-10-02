use crate::metadata::schema::traces;
use diesel::{AsChangeset, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Queryable, Selectable, PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = traces)]
pub struct DbTrace {
    pub trace_id: String,
    pub span_id: String,
    pub thread_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub operation_name: String,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub attribute: String,        // JSON stored as text
    pub run_id: Option<String>,
    pub project_id: Option<String>,
}

impl DbTrace {
    pub fn parse_attribute(&self) -> Option<HashMap<String, Value>> {
        serde_json::from_str(&self.attribute).ok()
    }
}

#[derive(Insertable, AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = traces)]
pub struct DbNewTrace {
    pub trace_id: String,
    pub span_id: String,
    pub thread_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub operation_name: String,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub attribute: String,
    pub run_id: Option<String>,
    pub project_id: Option<String>,
}

impl DbNewTrace {
    pub fn new(
        trace_id: String,
        span_id: String,
        thread_id: Option<String>,
        parent_span_id: Option<String>,
        operation_name: String,
        start_time_us: i64,
        finish_time_us: i64,
        attribute: HashMap<String, Value>,
        run_id: Option<String>,
        project_id: Option<String>,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            trace_id,
            span_id,
            thread_id,
            parent_span_id,
            operation_name,
            start_time_us,
            finish_time_us,
            attribute: serde_json::to_string(&attribute)?,
            run_id,
            project_id,
        })
    }
}
