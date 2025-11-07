use crate::threads::service::ThreadServiceImpl;
use std::sync::Arc;
use vllora_core::metadata::pool::DbPool;
use vllora_core::types::metadata::project::Project;
use vllora_core::types::threads::public_threads::PublicThreads;
use vllora_core::types::threads::related_threads::RelatedThreads;
use vllora_core::types::threads::service::ThreadService;
use vllora_core::types::threads::ThreadServiceWrapper;

pub mod service;

#[derive(Clone)]
pub struct ThreadImpl {
    #[allow(dead_code)]
    db_pool: DbPool,
    service: Arc<dyn ThreadService>,
}

impl ThreadImpl {
    pub fn new(db_pool: DbPool, project: Project) -> Self {
        Self {
            service: Arc::new(ThreadServiceImpl::new(db_pool.clone(), project.clone())),
            db_pool,
        }
    }
}

impl ThreadServiceWrapper for ThreadImpl {
    fn related_threads(&self) -> Arc<dyn RelatedThreads> {
        todo!()
    }

    fn public_threads(&self) -> Arc<dyn PublicThreads> {
        todo!()
    }

    fn service(&self) -> Arc<dyn ThreadService> {
        self.service.clone()
    }
}
