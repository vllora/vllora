use crate::metadata::schema::projects;
use crate::types::metadata::project::Project;
use crate::types::project_settings::ProjectSettings;
use chrono::{DateTime, NaiveDateTime};
use diesel::helper_types::AsSelect;
use diesel::helper_types::Select;
#[cfg(feature = "postgres")]
use diesel::pg::Pg;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::SelectableHelper;
use diesel::{AsChangeset, Insertable, QueryableByName, Selectable};
use diesel::{Identifiable, Queryable};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;
use vllora_llm::types::provider::ModelPrice;

#[derive(
    QueryableByName,
    Selectable,
    Queryable,
    PartialEq,
    Eq,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Default,
    Identifiable,
    AsChangeset,
)]
#[serde(crate = "serde")]
#[diesel(table_name = projects)]
pub struct DbProject {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub slug: String,
    pub settings: Option<String>,
    pub is_default: i32,
    pub archived_at: Option<String>,
    pub(crate) allowed_user_ids: Option<String>,
    pub private_model_prices: Option<String>,
}

#[cfg(feature = "sqlite")]
type All = Select<projects::table, AsSelect<DbProject, Sqlite>>;
#[cfg(feature = "postgres")]
type All = Select<projects::table, AsSelect<DbProject, Pg>>;

impl DbProject {
    pub fn all() -> All {
        diesel::QueryDsl::select(projects::table, DbProject::as_select())
    }

    #[diesel::dsl::auto_type(no_type_alias)]
    pub fn not_archived() -> _ {
        let all: All = DbProject::all();
        all.filter(projects::archived_at.is_null())
    }

    pub fn parse_settings(&self) -> Option<ProjectSettings> {
        self.settings.as_ref().and_then(|settings_str| {
            serde_json::from_str::<Value>(settings_str)
                .ok()
                .and_then(|v| serde_json::from_value(v).ok())
        })
    }

    pub fn is_user_allowed(&self, user_id: &str) -> bool {
        match self.allowed_user_ids.as_ref() {
            None => true,
            Some(s) if s.is_empty() => false,
            Some(s) => serde_json::from_str::<Vec<String>>(s)
                .map(|ids| ids.iter().any(|id| id == user_id))
                .unwrap_or(true),
        }
    }
}

impl From<DbProject> for Project {
    fn from(val: DbProject) -> Self {
        let id = Uuid::parse_str(&val.id).unwrap_or_else(|_| Uuid::nil());
        let created_at = parse_naive_datetime(&val.created_at);
        let updated_at = parse_naive_datetime(&val.updated_at);
        let archived_at = val.archived_at.as_deref().map(parse_naive_datetime);
        let settings = val
            .settings
            .as_deref()
            .and_then(|s| serde_json::from_str::<ProjectSettings>(s).ok());
        let private_model_prices = val
            .private_model_prices
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok());
        let allowed_user_ids = val
            .allowed_user_ids
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok());

        Project {
            id,
            name: val.name,
            description: val.description,
            created_at,
            updated_at,
            company_id: Uuid::nil(),
            slug: val.slug,
            settings,
            is_default: val.is_default != 0,
            archived_at,
            allowed_user_ids,
            private_model_prices,
        }
    }
}

fn parse_naive_datetime(value: &str) -> NaiveDateTime {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return dt.naive_utc();
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return dt;
    }
    DateTime::from_timestamp(0, 0).unwrap().naive_utc()
}

#[derive(Insertable, AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = projects)]
pub struct DbNewProject {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub slug: String,
    pub settings: Option<String>,
    pub is_default: Option<i32>,
}

#[derive(AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = projects)]
pub struct DbUpdateProject {
    pub name: String,
    pub description: Option<String>,
    pub settings: Option<String>,
    pub is_default: Option<i32>,
}

#[derive(AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = projects)]
pub struct DbUpdateProjectAllowedUserIds {
    pub allowed_user_ids: Option<Option<String>>,
}

#[derive(PartialEq, Debug, Serialize, Deserialize, Default, Clone)]
#[serde(crate = "serde")]
pub struct NewProjectDTO {
    pub name: String,
    pub description: Option<String>,
    pub settings: Option<Value>,
    pub private_model_prices: Option<HashMap<String, ModelPrice>>,
    pub usage_limit: Option<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateProjectDTO {
    pub name: Option<String>,
    pub description: Option<String>,
    pub settings: Option<Value>,
    pub is_default: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project() -> DbProject {
        DbProject {
            id: String::from("00000000-0000-0000-0000-000000000000"),
            name: String::from("Test Project"),
            description: None,
            created_at: String::from("1970-01-01T00:00:00Z"),
            updated_at: String::from("1970-01-01T00:00:00Z"),
            slug: String::from("test-project"),
            settings: None,
            is_default: 0,
            archived_at: None,
            allowed_user_ids: None,
            private_model_prices: None,
        }
    }

    #[test]
    fn test_is_user_allowed_some_allowed() {
        let mut project = test_project();
        project.allowed_user_ids = Some("[\"user1\",\"user2\"]".to_string());
        assert!(project.is_user_allowed("user1"));
        assert!(project.is_user_allowed("user2"));
        assert!(!project.is_user_allowed("user3"));
    }

    #[test]
    fn test_is_user_allowed_empty_list() {
        let mut project = test_project();
        project.allowed_user_ids = Some("[]".to_string());
        // No one is allowed
        assert!(!project.is_user_allowed("anyone"));
    }

    #[test]
    fn test_is_user_allowed_none() {
        let mut project = test_project();
        project.allowed_user_ids = None;
        // Everyone is allowed
        assert!(project.is_user_allowed("anyone"));
    }
}
