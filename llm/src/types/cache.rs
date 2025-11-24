use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseCacheOptions {
    pub expiration_time: Option<u32>,
    #[serde(flatten)]
    pub adapter: ResponseCacheAdapter,
}

impl Default for ResponseCacheOptions {
    fn default() -> Self {
        Self {
            expiration_time: Some(24 * 60 * 60),
            adapter: ResponseCacheAdapter::Exact,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ResponseCacheAdapter {
    Exact,
    Distance(DistanceCacheOptions),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistanceCacheOptions {
    pub min_similarity: f32,
}
