use crate::events::callback_handler::{
    CloudMessageEvent, GatewayCallbackHandlerFn, GatewayThreadEvent, MessageEventType,
    ThreadEventType,
};
use crate::llm_gateway::message_mapper::MessageMapper;
use crate::metadata::error::DatabaseError;
use crate::routing::RoutingStrategy;
use crate::types::gateway::{ChatCompletionMessage, ChatCompletionRequestWithTools, ToolCall};
use crate::types::message::MessageType;
use crate::types::project_settings::ProjectSettings;
use crate::types::threads::MessageThreadWithTitle;
use crate::types::threads::PageOptions;
use crate::types::threads::{InsertMessageResult, ThreadEntity};
use crate::types::threads::{Message, MessageContentType, MessageThread, MessageWithId};
use crate::GatewayError;
use std::sync::Arc;
use thiserror::Error;
use tracing::Span;

pub mod thread_entity;

#[derive(Error, Debug)]
pub enum HistoryError {
    #[error("Failed to create thread: {0}")]
    FailedToCreateThread(DatabaseError),

    #[error("Failed to fetch messages: {0}")]
    FailedToFetchMessages(DatabaseError),

    #[error("Failed to insert messages: {0}")]
    FailedToInsertMessages(DatabaseError),

    #[error("Failed to map message: {0}")]
    FailedToMapMessage(GatewayError),
}

#[derive(Clone, Debug)]
pub struct HistoryContext {
    pub model_name: String,
    pub user_id: String,
    pub thread_id: String,
}

/// Result of thread creation or retrieval
pub struct ThreadInfo {
    pub thread_id: String,
    pub thread: MessageThread,
    pub new_thread: bool,
}

/// Result of bulk message insertion
pub struct BulkInsertResult {
    pub inserted_messages: Vec<InsertMessageResult>,
    pub last_message_id: String,
}

/// Handles thread history management including creation and bulk message insertion
#[derive(Clone)]
pub struct ThreadHistoryManager {
    thread_entities: Arc<Box<dyn ThreadEntity>>,
    project_settings: Option<ProjectSettings>,
    project_id: String,
    cloud_callback_handler: GatewayCallbackHandlerFn,
}

impl ThreadHistoryManager {
    pub fn new(
        thread_entities: Arc<Box<dyn ThreadEntity>>,
        project_settings: Option<ProjectSettings>,
        project_id: String,
        cloud_callback_handler: &GatewayCallbackHandlerFn,
    ) -> Self {
        Self {
            thread_entities,
            project_settings,
            project_id,
            cloud_callback_handler: cloud_callback_handler.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_messages_for_request(
        &self,
        thread_id: &str,
        user_id: &str,
        project_slug: &str,
        is_public_thread: bool,
        thread_title: Option<String>,
        request: &ChatCompletionRequestWithTools<RoutingStrategy>,
        run_id: Option<String>,
    ) -> Result<BulkInsertResult, HistoryError> {
        let thread_info = MessageThread {
            id: thread_id.to_string(),
            model_name: request.request.model.to_string(),
            user_id: user_id.to_string(),
            project_id: project_slug.to_string(),
            is_public: is_public_thread,
            title: thread_title,
            description: None,
            keywords: None,
        };

        let thread = self.get_or_create_thread(thread_info).await?;

        let prev_messages = match thread.new_thread {
            true => vec![],
            false => self.fetch_thread_messages(thread.thread_id).await?,
        };

        // Determine which messages to insert
        let messages_to_insert = match prev_messages.is_empty() {
            false => vec![request.request.messages.last().unwrap().clone()],
            true => request.request.messages.clone(),
        };

        if let Some(last_new_message) = messages_to_insert.last() {
            let message = MessageMapper::map_completions_message_to_langdb_message(
                last_new_message,
                &request.request.model,
                user_id,
            )
            .map_err(HistoryError::FailedToMapMessage)?;

            if let Some(last_prev_message) = prev_messages.last() {
                if last_prev_message.message.is_content_identical(&message) {
                    let span = Span::current();
                    span.record("message_id", &last_prev_message.id);
                    return Ok(BulkInsertResult {
                        inserted_messages: vec![],
                        last_message_id: last_prev_message.id.clone(),
                    });
                }
            }
        }

        // Perform bulk message insertion
        let bulk_result = self
            .insert_messages_bulk(
                messages_to_insert,
                &request.request.model,
                user_id,
                thread_id,
            )
            .await?;

        if !bulk_result.inserted_messages.is_empty() {
            self.broadcast_message_event(
                MessageEventType::Created,
                thread_id,
                &bulk_result.last_message_id,
                &self.project_id,
                &self.thread_entities.get_tenant_name(),
                run_id,
            )
            .await;
        }

        Ok(bulk_result)
    }

    async fn broadcast_thread_event(
        &self,
        event_type: ThreadEventType,
        thread: &MessageThreadWithTitle,
    ) {
        let cloud_callback_handler = self.cloud_callback_handler.clone();
        let thread = thread.clone();
        // Run fetch thread in background
        tokio::spawn(async move {
            cloud_callback_handler
                .on_message(
                    GatewayThreadEvent {
                        event_type,
                        thread: thread.clone(),
                    }
                    .into(),
                )
                .await;
        });
    }

    async fn broadcast_message_event(
        &self,
        event_type: MessageEventType,
        thread_id: &str,
        message_id: &str,
        project_id: &str,
        tenant_name: &str,
        run_id: Option<String>,
    ) {
        let cloud_callback_handler = self.cloud_callback_handler.clone();

        let thread_id = thread_id.to_string();
        let message_id = message_id.to_string();
        let project_id = project_id.to_string();
        let tenant_name = tenant_name.to_string();
        let run_id = run_id.clone();

        // Run fetch thread in background
        tokio::spawn(async move {
            cloud_callback_handler
                .on_message(
                    CloudMessageEvent {
                        event_type,
                        thread_id,
                        message_id,
                        project_id,
                        tenant_name,
                        run_id,
                    }
                    .into(),
                )
                .await;
        });
    }

    /// Creates or retrieves a thread based on the provided thread info
    pub async fn get_or_create_thread(
        &self,
        thread_info: MessageThread,
    ) -> Result<ThreadInfo, HistoryError> {
        let thread_id = thread_info.id.clone();

        // Check if thread exists
        match self
            .thread_entities
            .get_thread_by_id(thread_id.clone())
            .await
        {
            Ok(thread) => Ok(ThreadInfo {
                thread_id: thread.id.clone(),
                thread,
                new_thread: false,
            }),
            Err(_) => {
                // Thread doesn't exist, create it
                self.thread_entities
                    .create_thread(thread_info.clone())
                    .await
                    .map_err(HistoryError::FailedToCreateThread)?;

                self.broadcast_thread_event(
                    ThreadEventType::Created,
                    &Into::<MessageThreadWithTitle>::into(thread_info.clone()),
                )
                .await;

                Ok(ThreadInfo {
                    thread_id: thread_info.id.clone(),
                    thread: thread_info,
                    new_thread: true,
                })
            }
        }
    }

    /// Fetches messages for a given thread
    pub async fn fetch_thread_messages(
        &self,
        thread_id: String,
    ) -> Result<Vec<MessageWithId>, HistoryError> {
        self.thread_entities
            .get_messages_by_thread_id(thread_id, PageOptions::default())
            .await
            .map_err(HistoryError::FailedToFetchMessages)
    }

    /// Inserts messages in bulk, optimizing database operations
    pub async fn insert_messages_bulk(
        &self,
        messages: Vec<ChatCompletionMessage>,
        model_name: &str,
        user_id: &str,
        thread_id: &str,
    ) -> Result<BulkInsertResult, HistoryError> {
        let span = Span::current();
        // Check if chat tracing is enabled
        if let Some(ProjectSettings {
            enabled_chat_tracing: false,
        }) = self.project_settings
        {
            return Ok(BulkInsertResult {
                inserted_messages: vec![],
                last_message_id: String::new(),
            });
        }

        // Convert ChatCompletionMessages to langdb Messages
        let mut langdb_messages = Vec::new();
        for message in messages {
            if message.role.as_str() != "system" {
                let mut langdb_message = MessageMapper::map_completions_message_to_langdb_message(
                    &message, model_name, user_id,
                )
                .map_err(HistoryError::FailedToMapMessage)?;

                langdb_message.thread_id = Some(thread_id.to_string());
                langdb_messages.push(langdb_message);
            }
        }

        if langdb_messages.is_empty() {
            return Ok(BulkInsertResult {
                inserted_messages: vec![],
                last_message_id: String::new(),
            });
        }

        // Perform bulk insertion
        let results = self.insert_messages_batch(langdb_messages).await?;

        let last_message_id = results
            .last()
            .map(|r| r.message_id.clone())
            .unwrap_or_default();

        // Record the last message ID in span
        span.record("message_id", &last_message_id);

        Ok(BulkInsertResult {
            inserted_messages: results,
            last_message_id,
        })
    }

    /// Internal method to perform batch message insertion
    async fn insert_messages_batch(
        &self,
        messages: Vec<Message>,
    ) -> Result<Vec<InsertMessageResult>, HistoryError> {
        if messages.is_empty() {
            return Ok(vec![]);
        }

        // Use the new bulk insert method for better performance
        self.thread_entities
            .insert_messages_bulk(
                messages,
                self.project_id.clone(),
                self.project_settings.clone(),
            )
            .await
            .map_err(|e| {
                tracing::error!("Error in bulk message insertion: {}", e);
                HistoryError::FailedToInsertMessages(e)
            })
    }

    /// Handles assistant message creation and insertion after completion
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_assistant_message(
        &self,
        content: String,
        tool_calls:Vec<ToolCall>,
        model_name: String,
        thread_id: Option<String>,
        user_id: String,
        span: &Option<Span>,
        run_id: Option<String>,
        predefined_message_id: Option<String>,
    ) -> Result<Option<InsertMessageResult>, HistoryError> {
        let Some(thread_id) = thread_id else {
            return Ok(None);
        };

        // Check if chat tracing is enabled
        if let Some(ProjectSettings {
            enabled_chat_tracing: false,
        }) = self.project_settings
        {
            return Ok(None);
        }

        let assistant_message = Message {
            model_name,
            thread_id: Some(thread_id.clone()),
            user_id,
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            content_array: vec![],
            content_type: MessageContentType::Text,
            r#type: MessageType::AIMessage,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_call_id: None,
        };

        match self
            .thread_entities
            .insert_message(
                assistant_message,
                self.project_id.clone(),
                self.project_settings.clone(),
                predefined_message_id,
            )
            .await
        {
            Ok(result) => {
                if let Some(ref r) = result {
                    self.broadcast_message_event(
                        MessageEventType::Created,
                        &thread_id,
                        &r.message_id,
                        &self.project_id,
                        &self.thread_entities.get_tenant_name(),
                        run_id,
                    )
                    .await;

                    if let Some(span) = span {
                        span.record("message_id", &r.message_id);
                    }
                }
                Ok(result)
            }
            Err(e) => {
                tracing::error!("Error storing assistant message: {}", e);
                Err(HistoryError::FailedToInsertMessages(e))
            }
        }
    }
}
