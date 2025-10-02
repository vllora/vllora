use crate::metadata::error::DatabaseError;
use crate::metadata::models::message::{DbMessage, DbNewMessage};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::messages;
use crate::types::gateway::ToolCall;
use crate::types::message::MessageType;
use crate::types::threads::PageOptions;
use crate::types::threads::{
    InsertMessageResult, Message, MessageContentPart, MessageContentType, MessageWithId,
};
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use uuid::Uuid;
use std::str::FromStr;

pub struct MessageService {
    db_pool: DbPool,
}

impl MessageService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn get_messages_by_thread_id(
        &self,
        thread_id: &str,
        page_options: PageOptions,
    ) -> Result<Vec<Message>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut query = DbMessage::by_thread_id(thread_id).into_boxed();

        // Apply ORDER BY
        for (column, order_type) in &page_options.order_by {
            match column.as_str() {
                "created_at" => {
                    query = match order_type {
                        crate::types::threads::PageOrderType::Asc => {
                            query.order(messages::created_at.asc())
                        }
                        crate::types::threads::PageOrderType::Desc => {
                            query.order(messages::created_at.desc())
                        }
                    };
                }
                _ => {
                    // Default to created_at if column not recognized
                    query = query.order(messages::created_at.desc());
                }
            }
        }

        // If no order_by specified, default to created_at DESC
        if page_options.order_by.is_empty() {
            query = query.order(messages::created_at.desc());
        }

        // Apply LIMIT
        if let Some(limit) = page_options.limit {
            query = query.limit(limit as i64);
        }

        // Apply OFFSET
        if let Some(offset) = page_options.offset {
            query = query.offset(offset as i64);
        }

        let db_messages: Vec<DbMessage> = query.load(&mut conn)?;

        Ok(db_messages
            .into_iter()
            .map(|m| self.db_message_to_message(m))
            .collect())
    }

    pub fn create_message(
        &self,
        message: Message,
        project_id: String,
        message_id: Option<String>,
    ) -> Result<InsertMessageResult, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let thread_id = match message.thread_id {
            Some(thread_id) => thread_id,
            None => {
                return Err(DatabaseError::InvalidArgument(
                    "Thread ID is required".to_string(),
                ))
            }
        };

        let new_message = DbNewMessage {
            id: message_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            model_name: Some(message.model_name),
            r#type: Some(message.r#type.to_string()),
            thread_id,
            user_id: Some(message.user_id),
            content_type: Some(message.content_type.to_string()),
            content: message.content,
            content_array: Some(
                serde_json::to_string(&message.content_array).unwrap_or_else(|_| "[]".to_string()),
            ),
            tool_call_id: message.tool_call_id,
            tool_calls: message
                .tool_calls
                .map(|tc| serde_json::to_string(&tc).unwrap_or_else(|_| "null".to_string())),
            tenant_id: None,
            project_id: Some(project_id),
        };

        diesel::insert_into(messages::table)
            .values(&new_message)
            .execute(&mut conn)?;

        Ok(InsertMessageResult {
            message_id: new_message.id,
            thread_id: new_message.thread_id,
        })
    }

    pub fn insert_many_messages(
        &self,
        messages: Vec<Message>,
        project_id: String,
    ) -> Result<Vec<InsertMessageResult>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let mut created_messages = Vec::new();

        for message in messages {
            let thread_id = match message.thread_id {
                Some(thread_id) => thread_id,
                None => continue,
            };

            let new_message = DbNewMessage {
                id: Uuid::new_v4().to_string(),
                model_name: Some(message.model_name),
                r#type: Some(message.r#type.to_string()),
                thread_id,
                user_id: Some(message.user_id),
                content_type: Some(message.content_type.to_string()),
                content: message.content,
                content_array: Some(
                    serde_json::to_string(&message.content_array)
                        .unwrap_or_else(|_| "[]".to_string()),
                ),
                tool_call_id: message.tool_call_id,
                tool_calls: message
                    .tool_calls
                    .map(|tc| serde_json::to_string(&tc).unwrap_or_else(|_| "null".to_string())),
                tenant_id: None,
                project_id: Some(project_id.clone()),
            };

            diesel::insert_into(messages::table)
                .values(&new_message)
                .execute(&mut conn)?;

            created_messages.push(InsertMessageResult {
                message_id: new_message.id,
                thread_id: new_message.thread_id,
            });
        }

        Ok(created_messages)
    }

    // Required methods for ThreadEntity interface
    pub fn get_by_thread_id(
        &self,
        thread_id: &str,
        page_options: PageOptions,
    ) -> Result<Vec<MessageWithId>, DatabaseError> {
        let messages = self.get_messages_by_thread_id(&thread_id, page_options)?;
        let messages_with_id = messages
            .into_iter()
            .map(|message| MessageWithId {
                id: Uuid::new_v4().to_string(), // Generate ID for each message
                message,
            })
            .collect();
        Ok(messages_with_id)
    }

    pub fn insert_many(
        &self,
        messages: Vec<Message>,
        project_id: String,
    ) -> Result<Vec<InsertMessageResult>, DatabaseError> {
        let created_messages = self.insert_many_messages(messages, project_id)?;
        let results = created_messages
            .into_iter()
            .map(|message| InsertMessageResult {
                message_id: Uuid::new_v4().to_string(), // Generate ID for each result
                thread_id: message.thread_id,
            })
            .collect();
        Ok(results)
    }

    pub fn insert_one(
        &self,
        message: Message,
        project_id: String,
        message_id: Option<String>,
    ) -> Result<Option<InsertMessageResult>, DatabaseError> {
        let created_message = self.create_message(message, project_id, message_id)?;
        Ok(Some(created_message))
    }

    fn db_message_to_message(&self, db_message: DbMessage) -> Message {
        let content_type = db_message
            .content_type
            .as_deref()
            .and_then(|ct| serde_json::from_str::<MessageContentType>(ct).ok())
            .unwrap_or(MessageContentType::Text);

        let message_type = db_message
            .r#type
            .as_deref()
            .and_then(|t| MessageType::from_str(t).ok())
            .unwrap_or(MessageType::HumanMessage);

        let content_array: Vec<MessageContentPart> = db_message
            .parse_content_array()
            .into_iter()
            .map(
                |(content_type, value, additional_options)| MessageContentPart {
                    r#type: serde_json::from_str(&content_type).unwrap_or(MessageContentType::Text),
                    value,
                    additional_options: additional_options
                        .and_then(|opt| serde_json::from_str(&opt).ok()),
                    cache_control: None,
                },
            )
            .collect();

        let tool_calls: Option<Vec<ToolCall>> = db_message
            .parse_tool_calls()
            .and_then(|tc| serde_json::from_value(tc).ok());

        Message {
            model_name: db_message.model_name.unwrap_or_default(),
            thread_id: db_message.thread_id,
            user_id: db_message.user_id.unwrap_or_default(),
            content_type,
            content: db_message.content,
            content_array,
            r#type: message_type,
            tool_call_id: db_message.tool_call_id,
            tool_calls,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_utils::setup_test_database;
    use crate::types::message::MessageType;
    use crate::types::threads::{Message, MessageContentPart, MessageContentType};

    fn create_test_message() -> Message {
        Message {
            model_name: "gpt-4".to_string(),
            thread_id: Some("test-thread-id".to_string()),
            user_id: "test-user-id".to_string(),
            content_type: MessageContentType::Text,
            content: Some("Hello, world!".to_string()),
            content_array: vec![MessageContentPart {
                r#type: MessageContentType::Text,
                value: "Hello, world!".to_string(),
                additional_options: None,
                cache_control: None,
            }],
            r#type: MessageType::HumanMessage,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    #[test]
    fn test_db_message_to_message() {
        let db_pool = setup_test_database();
        let service = MessageService::new(db_pool);

        let db_message = DbMessage {
            id: "test-message-id".to_string(),
            model_name: Some("gpt-4".to_string()),
            r#type: Some("human".to_string()),
            thread_id: Some("test-thread-id".to_string()),
            user_id: Some("test-user-id".to_string()),
            content_type: Some("text".to_string()),
            content: Some("Hello, world!".to_string()),
            content_array: r#"[["text", "Hello, world!", null]]"#.to_string(),
            tool_call_id: None,
            tool_calls: None,
            tenant_id: Some("test-tenant".to_string()),
            project_id: Some("test-project".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
        };

        let message = service.db_message_to_message(db_message);

        assert_eq!(message.model_name, "gpt-4");
        assert_eq!(message.thread_id, Some("test-thread-id".to_string()));
        assert_eq!(message.user_id, "test-user-id");
        assert_eq!(message.content, Some("Hello, world!".to_string()));
        assert_eq!(message.content_array.len(), 1);
        assert_eq!(message.content_array[0].value, "Hello, world!");
        assert_eq!(message.tool_call_id, None);
        assert_eq!(message.tool_calls, None);
    }

    #[test]
    fn test_create_message_conversion() {
        let db_pool = setup_test_database();
        let service = MessageService::new(db_pool);

        let message = create_test_message();

        let new_message = DbNewMessage {
            id: Some(Uuid::new_v4().to_string()),
            model_name: Some(message.model_name.clone()),
            r#type: Some(message.r#type.to_string()),
            thread_id: message.thread_id.clone(),
            user_id: Some(message.user_id.clone()),
            content_type: Some(message.content_type.to_string()),
            content: message.content.clone(),
            content_array: Some(
                serde_json::to_string(&message.content_array).unwrap_or_else(|_| "[]".to_string()),
            ),
            tool_call_id: message.tool_call_id.clone(),
            tool_calls: message
                .tool_calls
                .as_ref()
                .map(|tc| serde_json::to_string(tc).unwrap_or_else(|_| "null".to_string())),
            tenant_id: None,
            project_id: None,
        };

        assert_eq!(new_message.model_name, Some("gpt-4".to_string()));
        assert_eq!(new_message.thread_id, Some("test-thread-id".to_string()));
        assert_eq!(new_message.user_id, Some("test-user-id".to_string()));
        assert_eq!(new_message.content, Some("Hello, world!".to_string()));
        assert!(new_message.id.is_some());
    }
}
