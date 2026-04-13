use crate::metadata::schema::knowledge_sources;
use diesel::{AsChangeset, Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = knowledge_sources)]
#[serde(crate = "serde")]
pub struct DbKnowledgeSource {
    pub id: String,
    pub reference_id: Option<String>,
    pub workflow_id: String,
    pub name: String,
    pub description: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String,
    pub deleted_at: Option<String>,
    pub trace_bundle_id: Option<String>,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = knowledge_sources)]
pub struct DbNewKnowledgeSource {
    pub id: String,
    pub reference_id: Option<String>,
    pub workflow_id: String,
    pub name: String,
    pub description: Option<String>,
    pub metadata: Option<String>,
    pub trace_bundle_id: Option<String>,
}

impl DbNewKnowledgeSource {
    pub fn new(
        workflow_id: String,
        name: String,
        description: Option<String>,
        metadata: Option<String>,
        reference_id: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            reference_id,
            workflow_id,
            name,
            description,
            metadata,
            trace_bundle_id: None,
        }
    }
}

#[derive(Debug, AsChangeset, Clone, Default)]
#[diesel(table_name = knowledge_sources)]
pub struct DbUpdateKnowledgeSource {
    pub description: Option<String>,
    pub metadata: Option<String>,
    pub deleted_at: Option<String>,
    pub trace_bundle_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "serde")]
pub struct KnowledgeSource {
    pub id: String,
    pub reference_id: Option<String>,
    pub workflow_id: String,
    pub name: String,
    pub description: Option<String>,
    pub metadata: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_bundle_id: Option<String>,
    pub part: Vec<crate::metadata::models::knowledge_source_part::KnowledgeSourcePart>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(crate = "serde")]
pub struct NewKnowledgeSource {
    pub id: Option<String>,
    pub reference_id: Option<String>,
    pub workflow_id: String,
    pub name: String,
    pub description: Option<String>,
    pub metadata: Option<JsonValue>,
    #[serde(default)]
    pub trace_bundle_id: Option<String>,
    #[serde(default)]
    pub part: Vec<crate::metadata::models::knowledge_source_part::NewKnowledgeSourcePart>,
}

impl NewKnowledgeSource {
    pub fn into_models(
        self,
    ) -> Result<
        (
            DbNewKnowledgeSource,
            Vec<crate::metadata::models::knowledge_source_part::DbNewKnowledgeSourcePart>,
        ),
        serde_json::Error,
    > {
        let source_id = self.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let db_source = DbNewKnowledgeSource {
            id: source_id.clone(),
            reference_id: self.reference_id,
            workflow_id: self.workflow_id,
            name: self.name,
            description: self.description,
            metadata: self
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?,
            trace_bundle_id: self.trace_bundle_id,
        };

        let db_parts = self
            .part
            .into_iter()
            .map(|p| p.into_db_new(source_id.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok((db_source, db_parts))
    }
}

impl KnowledgeSource {
    pub fn from_models(
        source: DbKnowledgeSource,
        parts: Vec<crate::metadata::models::knowledge_source_part::DbKnowledgeSourcePart>,
    ) -> Result<Self, serde_json::Error> {
        let metadata = source
            .metadata
            .as_ref()
            .map(|s| serde_json::from_str::<JsonValue>(s))
            .transpose()?;

        let part = parts
            .into_iter()
            .map(crate::metadata::models::knowledge_source_part::KnowledgeSourcePart::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            id: source.id,
            reference_id: source.reference_id,
            workflow_id: source.workflow_id,
            name: source.name,
            description: source.description,
            metadata,
            trace_bundle_id: source.trace_bundle_id,
            part,
        })
    }
}
