use crate::metadata::schema::threads;
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
#[diesel(table_name = threads)]
pub struct DbThread {
    pub id: String,
    pub user_id: Option<String>,
    pub model_name: Option<String>,
    pub created_at: String,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub is_public: i32,
    pub description: Option<String>,
    pub keywords: String,
}

#[cfg(feature = "sqlite")]
type All = Select<threads::table, AsSelect<DbThread, Sqlite>>;
#[cfg(feature = "postgres")]
type All = Select<threads::table, AsSelect<DbThread, Pg>>;

impl DbThread {
    pub fn all() -> All {
        diesel::QueryDsl::select(threads::table, DbThread::as_select())
    }

    #[diesel::dsl::auto_type(no_type_alias)]
    pub fn by_project_id(project_id: &str) -> _ {
        let all: All = Self::all();
        all.filter(threads::project_id.eq(project_id))
    }

    pub fn parse_keywords(&self) -> Vec<String> {
        serde_json::from_str(&self.keywords).unwrap_or_default()
    }

    pub fn set_keywords(&mut self, keywords: Vec<String>) {
        self.keywords = serde_json::to_string(&keywords).unwrap_or_else(|_| "[]".to_string());
    }
}

#[derive(Insertable, AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = threads)]
pub struct DbNewThread {
    pub id: Option<String>,
    pub user_id: Option<String>,
    pub model_name: Option<String>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub is_public: Option<i32>,
    pub description: Option<String>,
    pub keywords: Option<String>,
}

#[derive(AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = threads)]
pub struct DbUpdateThread {
    pub user_id: Option<String>,
    pub model_name: Option<String>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub is_public: Option<i32>,
    pub description: Option<String>,
    pub keywords: Option<String>,
}

#[derive(PartialEq, Debug, Serialize, Deserialize, Default, Clone)]
#[serde(crate = "serde")]
pub struct NewThreadDTO {
    pub user_id: Option<String>,
    pub model_name: Option<String>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub is_public: Option<bool>,
    pub description: Option<String>,
    pub keywords: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateThreadDTO {
    pub user_id: Option<String>,
    pub model_name: Option<String>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub is_public: Option<bool>,
    pub description: Option<String>,
    pub keywords: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_thread() -> DbThread {
        DbThread {
            id: String::from("00000000-0000-0000-0000-000000000000"),
            user_id: Some(String::from("user123")),
            model_name: Some(String::from("gpt-4")),
            created_at: String::from("1970-01-01T00:00:00Z"),
            tenant_id: Some(String::from("tenant123")),
            project_id: Some(String::from("project123")),
            is_public: 0,
            description: Some(String::from("Test thread")),
            keywords: String::from("[\"test\", \"example\"]"),
        }
    }

    #[test]
    fn test_parse_keywords() {
        let thread = test_thread();
        let keywords = thread.parse_keywords();
        assert_eq!(keywords, vec!["test", "example"]);
    }

    #[test]
    fn test_set_keywords() {
        let mut thread = test_thread();
        thread.set_keywords(vec!["new".to_string(), "keywords".to_string()]);
        assert_eq!(thread.keywords, "[\"new\",\"keywords\"]");
    }
}
