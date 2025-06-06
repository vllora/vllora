use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptCacheOptions {
    pub expiration_time: Option<u32>,
    #[serde(flatten)]
    pub adapter: PromptCacheAdapter,
}

impl Default for PromptCacheOptions {
    fn default() -> Self {
        Self {
            expiration_time: Some(24 * 60 * 60),
            adapter: PromptCacheAdapter::Exact,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PromptCacheAdapter {
    Exact,
    Distance(DistanceCacheOptions),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistanceCacheOptions {
    pub min_similarity: f32,
}
