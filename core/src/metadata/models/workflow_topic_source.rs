use crate::metadata::schema::workflow_topic_sources;
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = workflow_topic_sources)]
#[serde(crate = "serde")]
pub struct DbWorkflowTopicSource {
    pub id: String,
    pub reference_id: Option<String>,
    pub workflow_id: String,
    pub topic_id: String,
    pub source_part_id: String,
    pub created_at: String,
}

#[derive(Debug, Insertable, Clone, Deserialize)]
#[diesel(table_name = workflow_topic_sources)]
#[serde(crate = "serde")]
pub struct DbNewWorkflowTopicSource {
    pub id: Option<String>,
    pub reference_id: Option<String>,
    pub workflow_id: String,
    pub topic_id: String,
    pub source_part_id: String,
}

impl DbNewWorkflowTopicSource {
    pub fn with_defaults(mut self) -> Self {
        if self.id.is_none() {
            self.id = Some(Uuid::new_v4().to_string());
        }
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(crate = "serde")]
pub struct TopicSourceCreateInput {
    pub id: Option<String>,
    pub reference_id: Option<String>,
    pub topic_identifier: String,
    #[serde(alias = "source_identifier")]
    pub part_identifier: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(crate = "serde")]
pub struct TopicSourceUpdateInput {
    pub identifier: String,
    pub reference_id: Option<String>,
    pub topic_identifier: Option<String>,
    #[serde(alias = "source_identifier")]
    pub part_identifier: Option<String>,
}
