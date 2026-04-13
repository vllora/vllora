use crate::metadata::schema::{workflow_record_scores, workflow_records};
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq)]
#[diesel(table_name = workflow_records)]
#[diesel(primary_key(id, workflow_id))]
#[serde(crate = "serde")]
pub struct DbWorkflowRecord {
    pub id: String,
    pub workflow_id: String,
    pub data: String,
    pub topic_id: Option<String>,
    pub span_id: Option<String>,
    pub is_generated: i32,
    pub source_record_id: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Insertable, Clone, Deserialize)]
#[diesel(table_name = workflow_records)]
#[serde(crate = "serde")]
pub struct DbNewWorkflowRecord {
    pub id: String,
    pub workflow_id: String,
    pub data: String,
    pub topic_id: Option<String>,
    pub span_id: Option<String>,
    pub is_generated: i32,
    pub source_record_id: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq)]
#[diesel(table_name = workflow_record_scores)]
#[serde(crate = "serde")]
pub struct DbWorkflowRecordScore {
    pub id: String,
    pub record_id: String,
    pub workflow_id: String,
    pub job_id: String,
    pub score_type: String,
    pub score: f32,
    pub created_at: String,
}

/// Lightweight aggregate stats for a workflow's records (no row data).
#[derive(Debug, Serialize, Clone)]
#[serde(crate = "serde")]
pub struct RecordsSummary {
    pub total: i64,
    pub with_topic: i64,
    pub generated: i64,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = workflow_record_scores)]
pub struct DbNewWorkflowRecordScore {
    pub id: String,
    pub record_id: String,
    pub workflow_id: String,
    pub job_id: String,
    pub score_type: String,
    pub score: f32,
}
