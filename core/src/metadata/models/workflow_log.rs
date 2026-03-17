use crate::metadata::schema::workflow_logs;
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = workflow_logs)]
#[serde(crate = "serde")]
pub struct DbWorkflowLog {
    pub id: String,
    pub workflow_id: String,
    pub target: Option<String>,
    pub log: String,
    pub created_at: String,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = workflow_logs)]
pub struct DbNewWorkflowLog {
    pub id: String,
    pub workflow_id: String,
    pub target: Option<String>,
    pub log: String,
}

impl DbNewWorkflowLog {
    pub fn new(workflow_id: String, target: Option<String>, log: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            workflow_id,
            target,
            log,
        }
    }
}
