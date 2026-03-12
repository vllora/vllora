use crate::metadata::schema::workflow_records;
use diesel::{Insertable, Queryable, Selectable, Identifiable};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq,
)]
#[diesel(table_name = workflow_records)]
#[serde(crate = "serde")]
pub struct DbWorkflowRecord {
    pub id: String,
    pub workflow_id: String,
    pub data: String,
    pub topic_id: Option<String>,
    pub span_id: Option<String>,
    pub is_generated: i32,
    pub source_record_id: Option<String>,
    pub dry_run_score: Option<f32>,
    pub finetune_score: Option<f32>,
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
