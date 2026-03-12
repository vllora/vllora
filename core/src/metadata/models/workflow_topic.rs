use crate::metadata::schema::workflow_topics;
use diesel::{Insertable, Queryable, Selectable, Identifiable};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = workflow_topics)]
#[serde(crate = "serde")]
pub struct DbWorkflowTopic {
    pub id: String,
    pub workflow_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub selected: i32,
    pub source_chunk_refs: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Insertable, Clone, Deserialize)]
#[diesel(table_name = workflow_topics)]
#[serde(crate = "serde")]
pub struct DbNewWorkflowTopic {
    pub id: String,
    pub workflow_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub selected: i32,
    pub source_chunk_refs: Option<String>,
}
