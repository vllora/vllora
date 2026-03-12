use crate::metadata::schema::workflow_topics;
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = workflow_topics)]
#[serde(crate = "serde")]
pub struct DbWorkflowTopic {
    pub id: String,
    pub reference_id: Option<String>,
    pub workflow_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub system_prompt: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Insertable, Clone, Deserialize)]
#[diesel(table_name = workflow_topics)]
#[serde(crate = "serde")]
pub struct DbNewWorkflowTopic {
    pub id: Option<String>,
    pub reference_id: Option<String>,
    pub workflow_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub system_prompt: Option<String>,
}

impl DbNewWorkflowTopic {
    pub fn with_defaults(mut self) -> Self {
        if self.id.is_none() {
            self.id = Some(Uuid::new_v4().to_string());
        }
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(crate = "serde")]
pub struct TopicUpdateInput {
    pub identifier: String,
    pub reference_id: Option<String>,
    pub name: Option<String>,
    pub parent_id: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Queryable, Selectable, Identifiable, PartialEq, Eq,
)]
#[diesel(table_name = workflow_topics)]
#[serde(crate = "serde")]
pub struct WorkflowTopicIdentifierMap {
    pub id: String,
    pub reference_id: Option<String>,
    pub workflow_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub system_prompt: Option<String>,
    pub created_at: String,
}
