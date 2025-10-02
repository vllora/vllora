use crate::metadata::error::DatabaseError;
use crate::metadata::pool::DbPool;
use crate::metadata::services::message::MessageService;
use crate::metadata::services::thread::ThreadService;
use crate::types::project_settings::ProjectSettings;
use crate::types::threads::InsertMessageResult;
use crate::types::threads::Message;
use crate::types::threads::MessageThread;
use crate::types::threads::MessageWithId;
use crate::types::threads::MessageWithMetrics;
use crate::types::threads::PageOptions;
use crate::types::threads::ThreadEntity;
use async_trait::async_trait;

pub struct ThreadEntityImpl {
    db_pool: DbPool,
}

impl ThreadEntityImpl {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }
}

#[async_trait]
impl ThreadEntity for ThreadEntityImpl {
    fn get_tenant_name(&self) -> String {
        "default".to_string()
    }

    async fn get_thread_by_id(&self, thread_id: String) -> Result<MessageThread, DatabaseError> {
        let thread_service = ThreadService::new(self.db_pool.clone());
        Ok(thread_service.get_thread_by_id(&thread_id)?)
    }

    async fn create_thread(&self, thread: MessageThread) -> Result<(), DatabaseError> {
        let thread_service = ThreadService::new(self.db_pool.clone());
        Ok(thread_service.create_thread(thread).map(|_| ())?)
    }

    async fn get_messages_by_thread_id(
        &self,
        thread_id: String,
        page_options: PageOptions,
    ) -> Result<Vec<MessageWithId>, DatabaseError> {
        let message_service = MessageService::new(self.db_pool.clone());
        Ok(message_service.get_by_thread_id(&thread_id, page_options)?)
    }

    async fn get_messages_with_metrics_by_thread_id(
        &self,
        _thread_id: String,
        _page_options: PageOptions,
    ) -> Result<Vec<MessageWithMetrics>, DatabaseError> {
        Ok(vec![])
    }

    async fn insert_messages_bulk(
        &self,
        messages: Vec<Message>,
        project_id: String,
        project_settings: Option<ProjectSettings>,
    ) -> Result<Vec<InsertMessageResult>, DatabaseError> {
        let message_service = MessageService::new(self.db_pool.clone());
        if let Some(project_settings) = project_settings {
            if project_settings.enabled_chat_tracing {
                return message_service.insert_many(messages, project_id);
            }
        }

        Ok(vec![])
    }

    async fn insert_message(
        &self,
        message: Message,
        project_id: String,
        project_settings: Option<ProjectSettings>,
        message_id: Option<String>,
    ) -> Result<Option<InsertMessageResult>, DatabaseError> {
        let message_service = MessageService::new(self.db_pool.clone());

        if let Some(project_settings) = project_settings {
            if project_settings.enabled_chat_tracing {
                return message_service.insert_one(message, project_id, message_id);
            }
        }

        Ok(None)
    }
}
