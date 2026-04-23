use crate::metadata::schema::jobs_logs;
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = jobs_logs)]
#[serde(crate = "serde")]
pub struct DbJobLog {
    pub id: String,
    pub job_id: String,
    pub level: String,
    pub event: String,
    pub payload_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = jobs_logs)]
pub struct DbNewJobLog {
    pub id: String,
    pub job_id: String,
    pub level: String,
    pub event: String,
    pub payload_json: Option<String>,
}

impl DbNewJobLog {
    pub fn new(job_id: String, level: String, event: String, payload_json: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            job_id,
            level,
            event,
            payload_json,
        }
    }
}
