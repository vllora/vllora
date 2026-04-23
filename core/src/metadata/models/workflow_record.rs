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
    /// Source URI the record was derived from (file://, hf://, s3://, etc.).
    /// Populated by `vllora finetune import-records` and the `record_generator`
    /// worker. Feature 001 traceability field; nullable for pre-migration rows.
    pub origin_uri: Option<String>,
    /// Source-system identifier (e.g., HuggingFace dataset name). Kept separate
    /// from `origin_uri` so queries like "all records from source X" work
    /// across URI revisions.
    pub origin_source_id: Option<String>,
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
    #[serde(default)]
    pub origin_uri: Option<String>,
    #[serde(default)]
    pub origin_source_id: Option<String>,
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

/// Record count for a single topic.
#[derive(Debug, Serialize, Clone, diesel::QueryableByName)]
#[serde(crate = "serde")]
pub struct TopicRecordCount {
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub topic_id: String,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub count: i64,
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
