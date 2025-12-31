use crate::metadata::schema::metrics;
use diesel::{AsChangeset, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Queryable, Selectable, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = metrics)]
pub struct DbMetric {
    pub metric_name: String,
    pub metric_type: String,
    pub value: f64,
    pub timestamp_us: i64,
    pub attributes: String, // JSON stored as text
    pub project_id: Option<String>,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

impl DbMetric {
    pub fn parse_attributes(&self) -> Option<HashMap<String, Value>> {
        serde_json::from_str(&self.attributes).ok()
    }
}

#[derive(Insertable, AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = metrics)]
pub struct DbNewMetric {
    pub metric_name: String,
    pub metric_type: String,
    pub value: f64,
    pub timestamp_us: i64,
    pub attributes: String,
    pub project_id: Option<String>,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

impl DbNewMetric {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        metric_name: String,
        metric_type: String,
        value: f64,
        timestamp_us: i64,
        attributes: HashMap<String, Value>,
        project_id: Option<String>,
        thread_id: Option<String>,
        run_id: Option<String>,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            metric_name,
            metric_type,
            value,
            timestamp_us,
            attributes: serde_json::to_string(&attributes)?,
            project_id,
            thread_id,
            run_id,
            trace_id,
            span_id,
        })
    }

    /// Create a new metric from OpenTelemetry metric data point
    pub fn from_metric_data_point(
        metric_name: String,
        metric_type: String,
        value: f64,
        timestamp_us: i64,
        attributes: Vec<(String, String)>,
        project_id: Option<String>,
        thread_id: Option<String>,
        run_id: Option<String>,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<Self, serde_json::Error> {
        let attributes_map: HashMap<String, Value> = attributes
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        
        Self::new(
            metric_name,
            metric_type,
            value,
            timestamp_us,
            attributes_map,
            project_id,
            thread_id,
            run_id,
            trace_id,
            span_id,
        )
    }
}
