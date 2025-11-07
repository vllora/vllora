use std::error::Error;

use crate::types::threads::MessageThreadWithTitle;

#[async_trait::async_trait]
pub trait RelatedThreads {
    async fn get_related_model_threads(
        &self,
        model_id: String,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<MessageThreadWithTitle>, Box<dyn Error + Send + Sync>>;
}
