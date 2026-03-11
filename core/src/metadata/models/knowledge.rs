use crate::metadata::schema::knowledge;
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = knowledge)]
#[serde(crate = "serde")]
pub struct DbKnowledge {
    pub id: String,
    pub name: String,
    pub workflow_id: String,
    pub metadata: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = knowledge)]
pub struct DbNewKnowledge {
    pub id: Option<String>,
    pub name: String,
    pub workflow_id: String,
    pub metadata: Option<String>,
    pub description: Option<String>,
}

impl DbNewKnowledge {
    pub fn new(
        workflow_id: String,
        name: String,
        metadata: Option<String>,
        description: Option<String>,
    ) -> Self {
        Self {
            id: Some(Uuid::new_v4().to_string()),
            name,
            workflow_id,
            metadata,
            description,
        }
    }
}
