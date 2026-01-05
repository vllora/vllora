use rmcp_actix_web::transport::StreamableHttpService;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

use crate::mcp::server::VlloraMcp;
use crate::metadata::services::project::ProjectServiceImpl;
use crate::metadata::{DatabaseService, DatabaseServiceTrait};
use crate::types::metadata::services::project::ProjectService;
use crate::types::metadata::services::trace::TraceService;
use actix_web::Scope;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

fn create_http_service<T: TraceService + Clone + Send + Sync + 'static>(
    session_manager: Arc<LocalSessionManager>,
    trace_service: T,
    project_slug: Option<String>,
) -> StreamableHttpService<VlloraMcp<T>> {
    let vllora_mcp = VlloraMcp::new(trace_service, project_slug);
    StreamableHttpService::builder()
        .service_factory(Arc::new(move || Ok(vllora_mcp.clone())))
        .session_manager(session_manager) // Session management
        .stateful_mode(true) // Enable sessions
        .sse_keep_alive(Duration::from_secs(30)) // Keep-alive pings
        .build()
}

pub fn attach_vllora_mcp<T: TraceService + DatabaseServiceTrait + Clone + Send + Sync + 'static>(
    scope: Scope,
    session_manager: Arc<LocalSessionManager>,
    database_service: &DatabaseService,
) -> Scope {
    let trace_service = database_service.init::<T>();

    // Get the default project slug
    let project_service = ProjectServiceImpl::new(database_service.db_pool().clone());
    let project_slug = project_service
        .get_default(Uuid::nil())
        .ok()
        .map(|p| p.slug);

    let http_service = create_http_service(session_manager, trace_service, project_slug);

    scope.service(http_service.clone().scope())
}
