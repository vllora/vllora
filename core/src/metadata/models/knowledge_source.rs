use crate::metadata::schema::knowledge_sources;
use diesel::{AsChangeset, Insertable, Queryable, Selectable, Identifiable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = knowledge_sources)]
#[serde(crate = "serde")]
pub struct DbKnowledgeSource {
    pub id: String,
    pub workflow_id: String,
    pub name: String,
    #[diesel(column_name = source_type)]
    #[serde(rename = "type")]
    pub source_type: String,
    pub content: Option<String>,
    pub extracted_content: Option<String>,
    pub status: String,
    pub progress: Option<String>,
    pub created_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = knowledge_sources)]
pub struct DbNewKnowledgeSource {
    pub id: String,
    pub workflow_id: String,
    pub name: String,
    #[diesel(column_name = source_type)]
    pub source_type: String,
    pub status: String,
    pub content: Option<String>,
    pub extracted_content: Option<String>,
}

impl DbNewKnowledgeSource {
    pub fn new(
        workflow_id: String,
        name: String,
        source_type: String,
        content: Option<String>,
        extracted_content: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            workflow_id,
            name,
            source_type,
            status: "pending".to_string(),
            content,
            extracted_content,
        }
    }
}

#[derive(Debug, AsChangeset, Clone, Default)]
#[diesel(table_name = knowledge_sources)]
pub struct DbUpdateKnowledgeSource {
    pub status: Option<String>,
    pub progress: Option<String>,
    pub extracted_content: Option<String>,
    pub deleted_at: Option<String>,
}
