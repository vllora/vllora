use rmcp_actix_web::transport::StreamableHttpService;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

use crate::mcp::server::VlloraMcp;
use crate::metadata::{DatabaseService, DatabaseServiceTrait};
use crate::types::metadata::services::trace::TraceService;
use actix_web::Scope;
use std::sync::Arc;
use std::time::Duration;

fn create_http_service<T: TraceService + Clone + Send + Sync + 'static>(
    session_manager: Arc<LocalSessionManager>,
    trace_service: T,
) -> StreamableHttpService<VlloraMcp<T>> {
    let vllora_mcp = VlloraMcp::new(trace_service);
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
    let http_service = create_http_service(session_manager, trace_service);

    // StreamableHttp-based calculator (cloned for each worker)
    scope.service(http_service.clone().scope())
}
