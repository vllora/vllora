use crate::metadata::error::DatabaseError;
use crate::metadata::services::thread::ThreadSpanQueryResult;
use crate::types::handlers::pagination::Pagination;
use serde::Serialize;

/// Extract the first N words from a text string
fn extract_first_n_words(text: &str, n: usize) -> String {
    text.split_whitespace()
        .take(n)
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Serialize)]
pub struct ThreadSpan {
    pub thread_id: String,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub run_ids: Vec<String>,
    pub input_models: Vec<String>,
    pub cost: f64,
    pub title: Option<String>,
}

impl From<ThreadSpanQueryResult> for ThreadSpan {
    fn from(result: ThreadSpanQueryResult) -> Self {
        let title = result.title.and_then(|t| {
            let trimmed = t.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(extract_first_n_words(trimmed, 10))
            }
        });

        ThreadSpan {
            thread_id: result.thread_id,
            start_time_us: result.start_time_us,
            finish_time_us: result.finish_time_us,
            run_ids: result
                .run_ids
                .map(|ids| {
                    ids.split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default(),
            input_models: result
                .input_models
                .map(|models| {
                    models
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default(),
            cost: result.cost,
            title,
        }
    }
}

#[derive(Serialize)]
pub struct PaginatedThreadSpans {
    pub data: Vec<ThreadSpan>,
    pub pagination: Pagination,
}

#[derive(thiserror::Error, Debug)]
pub enum ThreadServiceError {
    #[error("Failed to list threads: {0}")]
    FailedToListThreads(String),

    #[error("Failed to count threads: {0}")]
    FailedToCountThreads(String),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),
}

#[async_trait::async_trait]
pub trait ThreadService {
    async fn list_threads(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<PaginatedThreadSpans, ThreadServiceError>;
}
