use crate::metadata::schema::messages;
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
#[diesel(table_name = messages)]
pub struct DbMessage {
    pub id: String,
    pub model_name: Option<String>,
    pub r#type: Option<String>,
    pub thread_id: Option<String>,
    pub user_id: Option<String>,
    pub content_type: Option<String>,
    pub content: Option<String>,
    pub content_array: String,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<String>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub created_at: String,
}

#[cfg(feature = "sqlite")]
type All = Select<messages::table, AsSelect<DbMessage, Sqlite>>;
#[cfg(feature = "postgres")]
type All = Select<messages::table, AsSelect<DbMessage, Pg>>;

impl DbMessage {
    pub fn all() -> All {
        diesel::QueryDsl::select(messages::table, DbMessage::as_select())
    }

    #[diesel::dsl::auto_type(no_type_alias)]
    pub fn by_thread_id(thread_id: &str) -> _ {
        let all: All = Self::all();
        all.filter(messages::thread_id.eq(thread_id))
    }

    #[diesel::dsl::auto_type(no_type_alias)]
    pub fn by_user_id(user_id: &str) -> _ {
        let all: All = Self::all();
        all.filter(messages::user_id.eq(user_id))
    }

    #[diesel::dsl::auto_type(no_type_alias)]
    pub fn by_project_id(project_id: &str) -> _ {
        let all: All = Self::all();
        all.filter(messages::project_id.eq(project_id))
    }

    #[diesel::dsl::auto_type(no_type_alias)]
    pub fn by_type(message_type: &str) -> _ {
        let all: All = Self::all();
        all.filter(messages::r#type.eq(message_type))
    }

    pub fn parse_content_array(&self) -> Vec<(String, String, Option<String>)> {
        serde_json::from_str(&self.content_array).unwrap_or_default()
    }

    pub fn set_content_array(&mut self, content_array: Vec<(String, String, Option<String>)>) {
        self.content_array =
            serde_json::to_string(&content_array).unwrap_or_else(|_| "[]".to_string());
    }

    pub fn parse_tool_calls(&self) -> Option<Value> {
        self.tool_calls
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
    }

    pub fn set_tool_calls(&mut self, tool_calls: Option<Value>) {
        self.tool_calls = tool_calls.and_then(|v| serde_json::to_string(&v).ok());
    }
}

#[derive(Insertable, AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = messages)]
pub struct DbNewMessage {
    pub id: String,
    pub model_name: Option<String>,
    pub r#type: Option<String>,
    pub thread_id: String,
    pub user_id: Option<String>,
    pub content_type: Option<String>,
    pub content: Option<String>,
    pub content_array: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<String>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub created_at: String,
}

#[derive(AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = messages)]
pub struct DbUpdateMessage {
    pub model_name: Option<String>,
    pub r#type: Option<String>,
    pub thread_id: Option<String>,
    pub user_id: Option<String>,
    pub content_type: Option<String>,
    pub content: Option<String>,
    pub content_array: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<String>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(PartialEq, Debug, Serialize, Deserialize, Default, Clone)]
#[serde(crate = "serde")]
pub struct NewMessageDTO {
    pub model_name: Option<String>,
    pub r#type: Option<String>,
    pub thread_id: Option<String>,
    pub user_id: Option<String>,
    pub content_type: Option<String>,
    pub content: Option<String>,
    pub content_array: Option<Vec<(String, String, Option<String>)>>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Value>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateMessageDTO {
    pub model_name: Option<String>,
    pub r#type: Option<String>,
    pub thread_id: Option<String>,
    pub user_id: Option<String>,
    pub content_type: Option<String>,
    pub content: Option<String>,
    pub content_array: Option<Vec<(String, String, Option<String>)>>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Value>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_message() -> DbMessage {
        DbMessage {
            id: String::from("00000000-0000-0000-0000-000000000000"),
            model_name: Some(String::from("gpt-4")),
            r#type: Some(String::from("user")),
            thread_id: Some(String::from("thread123")),
            user_id: Some(String::from("user123")),
            content_type: Some(String::from("text")),
            content: Some(String::from("Hello, world!")),
            content_array: String::from(r#"[["text", "Hello, world!", null]]"#),
            tool_call_id: None,
            tool_calls: None,
            tenant_id: Some(String::from("tenant123")),
            project_id: Some(String::from("project123")),
            created_at: String::from("1970-01-01T00:00:00Z"),
        }
    }

    #[test]
    fn test_parse_content_array() {
        let message = test_message();
        let content_array = message.parse_content_array();
        assert_eq!(content_array.len(), 1);
        assert_eq!(
            content_array[0],
            ("text".to_string(), "Hello, world!".to_string(), None)
        );
    }

    #[test]
    fn test_set_content_array() {
        let mut message = test_message();
        let new_content = vec![(
            "text".to_string(),
            "New content".to_string(),
            Some("role".to_string()),
        )];
        message.set_content_array(new_content);
        assert_eq!(message.content_array, r#"[["text","New content","role"]]"#);
    }

    #[test]
    fn test_parse_tool_calls() {
        let mut message = test_message();
        message.tool_calls = Some(r#"{"function": "test", "args": {}}"#.to_string());
        let tool_calls = message.parse_tool_calls();
        assert!(tool_calls.is_some());
    }
}
