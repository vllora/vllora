use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::thread::ThreadService as DatabaseThreadService;
use vllora_core::types::handlers::pagination::Pagination;
use vllora_core::types::metadata::project::Project;
use vllora_core::types::threads::service::PaginatedThreadSpans;
use vllora_core::types::threads::service::ThreadService;
use vllora_core::types::threads::service::ThreadServiceError;
use vllora_core::types::threads::service::ThreadSpan;

pub struct ThreadServiceImpl {
    db_pool: DbPool,
    project: Project,
}

impl ThreadServiceImpl {
    pub fn new(db_pool: DbPool, project: Project) -> Self {
        Self { db_pool, project }
    }
}

#[async_trait::async_trait]
impl ThreadService for ThreadServiceImpl {
    async fn list_threads(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<PaginatedThreadSpans, ThreadServiceError> {
        let thread_service = DatabaseThreadService::new(self.db_pool.clone());

        // Query thread spans using the service
        let results = thread_service
            .list_thread_spans(&self.project.slug, limit, offset)
            .map_err(|e| ThreadServiceError::FailedToListThreads(e.to_string()))?;

        // Count total threads
        let total = thread_service
            .count_thread_spans(&self.project.slug)
            .map_err(|e| ThreadServiceError::FailedToCountThreads(e.to_string()))?;

        // Convert results to ThreadSpan
        let data: Vec<ThreadSpan> = results.into_iter().map(ThreadSpan::from).collect();

        Ok(PaginatedThreadSpans {
            data,
            pagination: Pagination {
                total,
                limit,
                offset,
            },
        })
    }
}
