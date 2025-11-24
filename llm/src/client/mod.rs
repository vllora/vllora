pub mod completions;
pub mod error;
pub mod message_mapper;
pub mod tools;

use std::sync::Arc;

use crate::client::completions::CompletionsClient;
pub use crate::types::instance::DummyModelInstance;
pub use crate::types::instance::ModelInstance;

pub const DEFAULT_MAX_RETRIES: u32 = 0;

pub struct VlloraLLMClient {
    instance: Arc<Box<dyn ModelInstance>>,
}

impl Default for VlloraLLMClient {
    fn default() -> Self {
        Self::new()
    }
}

impl VlloraLLMClient {
    pub fn new() -> Self {
        Self {
            instance: Arc::new(Box::new(DummyModelInstance {})),
        }
    }

    pub fn new_with_instance(instance: Arc<Box<dyn ModelInstance>>) -> Self {
        Self { instance }
    }

    pub fn completions(&self) -> CompletionsClient {
        CompletionsClient::new(self.instance.clone())
    }
}
