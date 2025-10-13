use crate::metadata::schema::project_model_restrictions;
use crate::types::metadata::tag_type::TagType;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(
    Queryable,
    Selectable,
    Identifiable,
    AsChangeset,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    PartialEq,
)]
#[diesel(table_name = project_model_restrictions)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct ProjectModelRestriction {
    pub id: String,
    pub project_id: String,
    pub tag_type: TagType,
    pub tag: String,
    #[serde(skip_serializing, skip_deserializing)]
    allowed_models: String,
    #[serde(skip_serializing, skip_deserializing)]
    disallowed_models: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ProjectModelRestriction {
    pub fn allowed_models(&self) -> Vec<String> {
        serde_json::from_str(&self.allowed_models).unwrap_or_default()
    }

    pub fn disallowed_models(&self) -> Vec<String> {
        serde_json::from_str(&self.disallowed_models).unwrap_or_default()
    }

    pub fn set_allowed_models(&mut self, models: Vec<String>) {
        self.allowed_models = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
    }

    pub fn set_disallowed_models(&mut self, models: Vec<String>) {
        self.disallowed_models =
            serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProjectModelRestrictionWithModels {
    pub id: String,
    pub project_id: String,
    pub tag_type: TagType,
    pub tag: String,
    pub allowed_models: Vec<String>,
    pub disallowed_models: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ProjectModelRestriction> for ProjectModelRestrictionWithModels {
    fn from(restriction: ProjectModelRestriction) -> Self {
        let allowed = restriction.allowed_models();
        let disallowed = restriction.disallowed_models();
        
        ProjectModelRestrictionWithModels {
            id: restriction.id,
            project_id: restriction.project_id,
            tag_type: restriction.tag_type,
            tag: restriction.tag,
            allowed_models: allowed,
            disallowed_models: disallowed,
            created_at: restriction.created_at,
            updated_at: restriction.updated_at,
        }
    }
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = project_model_restrictions)]
pub struct CreateProjectModelRestriction {
    pub id: String,
    pub project_id: String,
    pub tag_type: TagType,
    pub tag: String,
    pub allowed_models: String,
    pub disallowed_models: String,
    pub created_at: String,
    pub updated_at: String,
}

impl CreateProjectModelRestriction {
    pub fn new(
        project_id: String,
        tag_type: TagType,
        tag: String,
        allowed_models: Vec<String>,
        disallowed_models: Vec<String>,
    ) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let id = uuid::Uuid::new_v4().to_string();

        CreateProjectModelRestriction {
            id,
            project_id,
            tag_type,
            tag,
            allowed_models: serde_json::to_string(&allowed_models)
                .unwrap_or_else(|_| "[]".to_string()),
            disallowed_models: serde_json::to_string(&disallowed_models)
                .unwrap_or_else(|_| "[]".to_string()),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(AsChangeset, Debug, Clone)]
#[diesel(table_name = project_model_restrictions)]
pub struct UpdateProjectModelRestriction {
    pub allowed_models: Option<String>,
    pub disallowed_models: Option<String>,
    pub updated_at: String,
}

impl UpdateProjectModelRestriction {
    pub fn new(allowed_models: Option<Vec<String>>, disallowed_models: Option<Vec<String>>) -> Self {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        UpdateProjectModelRestriction {
            allowed_models: allowed_models
                .map(|models| serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string())),
            disallowed_models: disallowed_models
                .map(|models| serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string())),
            updated_at: now,
        }
    }
}

