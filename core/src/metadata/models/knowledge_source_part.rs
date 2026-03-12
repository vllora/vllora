use crate::metadata::schema::knowledge_source_parts;
use diesel::{Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(crate = "serde", rename_all = "lowercase")]
pub enum KnowledgePartType {
    Text,
    Image,
    Table,
}

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = knowledge_source_parts)]
#[serde(crate = "serde")]
pub struct DbKnowledgeSourcePart {
    pub id: String,
    pub source_id: String,
    #[diesel(column_name = part_type)]
    #[serde(rename = "type")]
    pub part_type: String,
    pub content: String,
    pub content_metadata: Option<String>,
    pub title: Option<String>,
    pub extraction_path: Option<String>,
    pub extraction_metadata: Option<String>,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = knowledge_source_parts)]
pub struct DbNewKnowledgeSourcePart {
    pub id: Option<String>,
    pub source_id: String,
    #[diesel(column_name = part_type)]
    pub part_type: String,
    pub content: String,
    pub content_metadata: Option<String>,
    pub title: Option<String>,
    pub extraction_path: Option<String>,
    pub extraction_metadata: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "serde")]
pub struct KnowledgeSourcePart {
    pub id: String,
    pub source_id: String,
    #[serde(rename = "type")]
    pub part_type: KnowledgePartType,
    pub content: String,
    pub content_metadata: Option<JsonValue>,
    pub title: Option<String>,
    pub extraction_path: Option<String>,
    pub extraction_metadata: Option<JsonValue>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "serde")]
pub struct NewKnowledgeSourcePart {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub part_type: KnowledgePartType,
    pub content: String,
    pub content_metadata: Option<JsonValue>,
    pub title: Option<String>,
    pub extraction_path: Option<String>,
    pub extraction_metadata: Option<JsonValue>,
}

impl NewKnowledgeSourcePart {
    pub fn into_db_new(self, source_id: String) -> Result<DbNewKnowledgeSourcePart, serde_json::Error> {
        Ok(DbNewKnowledgeSourcePart {
            id: self.id.or_else(|| Some(Uuid::new_v4().to_string())),
            source_id,
            part_type: match self.part_type {
                KnowledgePartType::Text => "text".to_string(),
                KnowledgePartType::Image => "image".to_string(),
                KnowledgePartType::Table => "table".to_string(),
            },
            content: self.content,
            content_metadata: self
                .content_metadata
                .map(|v| serde_json::to_string(&v))
                .transpose()?,
            title: self.title,
            extraction_path: self.extraction_path,
            extraction_metadata: self
                .extraction_metadata
                .map(|v| serde_json::to_string(&v))
                .transpose()?,
        })
    }
}

impl TryFrom<DbKnowledgeSourcePart> for KnowledgeSourcePart {
    type Error = serde_json::Error;

    fn try_from(value: DbKnowledgeSourcePart) -> Result<Self, Self::Error> {
        let part_type = match value.part_type.as_str() {
            "text" => KnowledgePartType::Text,
            "image" => KnowledgePartType::Image,
            "table" => KnowledgePartType::Table,
            _ => KnowledgePartType::Text,
        };

        Ok(Self {
            id: value.id,
            source_id: value.source_id,
            part_type,
            content: value.content,
            content_metadata: value
                .content_metadata
                .as_ref()
                .map(|s| serde_json::from_str::<JsonValue>(s))
                .transpose()?,
            title: value.title,
            extraction_path: value.extraction_path,
            extraction_metadata: value
                .extraction_metadata
                .as_ref()
                .map(|s| serde_json::from_str::<JsonValue>(s))
                .transpose()?,
        })
    }
}
