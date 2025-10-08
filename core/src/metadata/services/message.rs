use crate::metadata::error::DatabaseError;
use crate::metadata::models::message::{DbMessage, DbNewMessage};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::messages;

#[cfg(feature = "postgres")]
use diesel::pg::PgConnection as Connection;
#[cfg(feature = "sqlite")]
use diesel::sqlite::SqliteConnection as Connection;

use crate::types::gateway::ToolCall;
use crate::types::message::MessageType;
use crate::types::threads::PageOptions;
use crate::types::threads::{
    InsertMessageResult, Message, MessageContentPart, MessageContentType, MessageWithAllMetrics,
    MessageWithId,
};
use diesel::sql_types::{BigInt, Double, Nullable, Text};
use diesel::OptionalExtension;
use diesel::{ExpressionMethods, QueryDsl, QueryableByName, RunQueryDsl};
use std::str::FromStr;
use uuid::Uuid;

#[derive(QueryableByName)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct MessageMetricsQueryResult {
    #[diesel(sql_type = Text)]
    message_id: String,
    #[diesel(sql_type = Nullable<Double>)]
    cost: Option<f64>,
    #[diesel(sql_type = Nullable<BigInt>)]
    duration: Option<i64>,
    #[diesel(sql_type = BigInt)]
    start_time_us: i64,
    #[diesel(sql_type = Nullable<BigInt>)]
    ttft: Option<i64>,
    #[diesel(sql_type = Nullable<Text>)]
    usage: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    run_id: Option<String>,
    #[diesel(sql_type = Text)]
    trace_id: String,
    #[diesel(sql_type = Text)]
    span_id: String,
}

pub struct MessageService {
    db_pool: DbPool,
}

impl MessageService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    fn get_message_metrics_from_traces(
        &self,
        conn: &mut Connection,
        thread_id: &str,
        message_id_filter: Option<&str>,
    ) -> Result<Vec<MessageMetricsQueryResult>, DatabaseError> {
        let where_clause = if let Some(msg_id) = message_id_filter {
            format!(
                "WHERE thread_id = '{}' AND json_extract(attribute, '$.message_id') = '{}'",
                thread_id, msg_id
            )
        } else {
            format!(
                "WHERE thread_id = '{}' AND json_extract(attribute, '$.message_id') IS NOT NULL",
                thread_id
            )
        };

        let sql = format!(
            r#"
            SELECT json_extract(attribute, '$.message_id') as message_id,
                   CAST(json_extract(attribute, '$.cost') as float) as cost,
                   finish_time_us - start_time_us as duration,
                   start_time_us,
                   CAST(json_extract(attribute, '$.ttft') as int) as ttft,
                   json_extract(attribute, '$.usage') as usage,
                   run_id,
                   trace_id,
                   span_id
            FROM traces
            {}
            ORDER BY start_time_us ASC
            "#,
            where_clause
        );

        diesel::sql_query(sql)
            .load::<MessageMetricsQueryResult>(conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn get_messages_by_thread_id(
        &self,
        thread_id: &str,
        page_options: PageOptions,
    ) -> Result<Vec<MessageWithId>, DatabaseError> {
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

        let created_at = chrono::Utc::now();
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
            created_at: created_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
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

        let mut created_at = chrono::Utc::now();
        let mut new_messages = vec![];

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
                created_at: created_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            };

            created_messages.push(InsertMessageResult {
                message_id: new_message.id.clone(),
                thread_id: new_message.thread_id.clone(),
            });
            new_messages.push(new_message);

            // Add 1 nanosecond to the created_at time to avoid duplicate timestamps
            created_at = created_at
                .checked_add_signed(chrono::Duration::microseconds(1))
                .unwrap();
        }

        diesel::insert_into(messages::table)
            .values(&new_messages)
            .execute(&mut conn)?;

        Ok(created_messages)
    }

    pub fn insert_many(
        &self,
        messages: Vec<Message>,
        project_id: String,
    ) -> Result<Vec<InsertMessageResult>, DatabaseError> {
        self.insert_many_messages(messages, project_id)
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

    pub fn get_message_by_id(
        &self,
        message_id: &str,
    ) -> Result<Option<MessageWithId>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let db_message = DbMessage::all()
            .filter(messages::id.eq(message_id))
            .first::<DbMessage>(&mut conn)
            .optional()?;

        Ok(db_message.map(|m| self.db_message_to_message(m)))
    }

    fn db_message_to_message(&self, db_message: DbMessage) -> MessageWithId {
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

        MessageWithId {
            id: db_message.id,
            message: Message {
                model_name: db_message.model_name.unwrap_or_default(),
                thread_id: db_message.thread_id,
                user_id: db_message.user_id.unwrap_or_default(),
                content_type,
                content: db_message.content,
                content_array,
                r#type: message_type,
                tool_call_id: db_message.tool_call_id,
                tool_calls,
                created_at: Some(db_message.created_at),
            },
        }
    }

    pub fn get_thread_message_with_metrics(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> Result<MessageWithAllMetrics, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let results =
            self.get_message_metrics_from_traces(&mut conn, thread_id, Some(message_id))?;

        // Get the specific message
        let message = self.get_message_by_id(message_id)?.ok_or_else(|| {
            DatabaseError::InvalidArgument(format!("Message {} not found", message_id))
        })?;

        // Collect all metrics for this message
        let mut metrics: Vec<crate::types::threads::MessageMetrics> = results
            .into_iter()
            .filter_map(|result| {
                let usage: Option<crate::types::gateway::CompletionModelUsage> =
                    result.usage.and_then(|u| serde_json::from_str(&u).ok());

                let metric = crate::types::threads::MessageMetrics {
                    ttft: result.ttft.map(|t| t as u64),
                    usage,
                    duration: result.duration.map(|d| d as u64),
                    run_id: result.run_id,
                    trace_id: Some(result.trace_id),
                    span_id: Some(result.span_id),
                    start_time_us: Some(result.start_time_us as u64),
                    cost: result.cost,
                };

                // Only include metrics with non-empty run_id
                metric.run_id.as_ref().filter(|id| !id.is_empty())?;
                Some(metric)
            })
            .collect();

        // Sort metrics by start_time_us (ascending)
        metrics.sort_by_key(|m| m.start_time_us.unwrap_or(u64::MAX));

        let result = MessageWithAllMetrics {
            message: message.message,
            id: message.id,
            metrics,
        };

        Ok(result)
    }

    pub fn get_thread_messages_with_metrics(
        &self,
        thread_id: &str,
        page_options: PageOptions,
    ) -> Result<Vec<MessageWithAllMetrics>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let results = self.get_message_metrics_from_traces(&mut conn, thread_id, None)?;

        // Get all messages for this thread
        let messages = self.get_messages_by_thread_id(thread_id, page_options)?;

        // Create a map of message_id to metrics for quick lookup
        let mut metrics_map: std::collections::HashMap<
            String,
            Vec<crate::types::threads::MessageMetrics>,
        > = std::collections::HashMap::new();

        for result in results {
            let usage: Option<crate::types::gateway::CompletionModelUsage> =
                result.usage.and_then(|u| serde_json::from_str(&u).ok());

            let metrics = crate::types::threads::MessageMetrics {
                ttft: result.ttft.map(|t| t as u64),
                usage,
                duration: result.duration.map(|d| d as u64),
                run_id: result.run_id,
                trace_id: Some(result.trace_id),
                span_id: Some(result.span_id),
                start_time_us: Some(result.start_time_us as u64),
                cost: result.cost,
            };

            // Only add metrics with non-empty run_id
            if let Some(run_id) = metrics.run_id.as_ref() {
                if !run_id.is_empty() {
                    metrics_map
                        .entry(result.message_id)
                        .or_default()
                        .push(metrics);
                }
            }
        }

        // Sort metrics by start_time_us (ascending), putting None at the end
        for metrics_list in metrics_map.values_mut() {
            metrics_list.sort_by_key(|m| m.start_time_us.unwrap_or(u64::MAX));
        }

        // Build result starting from messages, attaching metrics if available
        let result: Vec<MessageWithAllMetrics> = messages
            .into_iter()
            .map(|message| {
                let metrics = metrics_map.remove(&message.id).unwrap_or_default();
                MessageWithAllMetrics {
                    message: message.message,
                    id: message.id,
                    metrics,
                }
            })
            .collect();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_utils::setup_test_database;
    use crate::types::threads::{Message, MessageContentType};

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

        assert_eq!(message.id, "test-message-id");
        assert_eq!(message.message.model_name, "gpt-4");
        assert_eq!(
            message.message.thread_id,
            Some("test-thread-id".to_string())
        );
        assert_eq!(message.message.user_id, "test-user-id");
        assert_eq!(message.message.content, Some("Hello, world!".to_string()));
        assert_eq!(message.message.content_array.len(), 1);
        assert_eq!(message.message.content_array[0].value, "Hello, world!");
        assert_eq!(message.message.tool_call_id, None);
        assert_eq!(message.message.tool_calls, None);
    }

    #[test]
    fn test_get_message_by_id() {
        let db_pool = setup_test_database();
        let service = MessageService::new(db_pool);

        let msg = Message {
            model_name: "gpt-4".to_string(),
            thread_id: Some("test-thread-id".to_string()),
            user_id: "test-user-id".to_string(),
            content_type: MessageContentType::Text,
            content: Some("Hello, world!".to_string()),
            content_array: vec![],
            r#type: MessageType::HumanMessage,
            tool_call_id: None,
            tool_calls: None,
            created_at: None,
        };

        let predefined_id = Some("test-message-id-123".to_string());
        let res = service
            .insert_one(msg, "test-project".to_string(), predefined_id.clone())
            .expect("insert should succeed")
            .expect("insert should return result");

        assert_eq!(res.message_id, predefined_id.clone().unwrap());

        let fetched = service
            .get_message_by_id(&res.message_id)
            .expect("fetch by id should succeed")
            .expect("fetch by id should return result");

        assert_eq!(fetched.id, res.message_id);
        assert_eq!(fetched.message.user_id, "test-user-id");
        assert_eq!(
            fetched.message.thread_id,
            Some("test-thread-id".to_string())
        );
        assert_eq!(fetched.message.content, Some("Hello, world!".to_string()));
    }
}
