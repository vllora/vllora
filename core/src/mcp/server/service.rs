use rmcp_actix_web::transport::StreamableHttpService;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

use crate::mcp::server::VlloraMcp;
use crate::metadata::services::trace::TraceServiceImpl;
use actix_web::Scope;
use std::sync::Arc;
use std::time::Duration;

fn create_http_service(
    session_manager: Arc<LocalSessionManager>,
    trace_service: TraceServiceImpl,
) -> StreamableHttpService<VlloraMcp> {
    let trace_service = trace_service.clone();
    let vllora_mcp = VlloraMcp::new(trace_service);
    StreamableHttpService::builder()
        .service_factory(Arc::new(move || Ok(vllora_mcp.clone())))
        .session_manager(session_manager) // Session management
        .stateful_mode(true) // Enable sessions
        .sse_keep_alive(Duration::from_secs(30)) // Keep-alive pings
        .build()
}

pub fn attach_vllora_mcp(
    scope: Scope,
    session_manager: Arc<LocalSessionManager>,
    trace_service: TraceServiceImpl,
) -> Scope {
    let http_service = create_http_service(session_manager, trace_service);

    // StreamableHttp-based calculator (cloned for each worker)
    scope.service(http_service.clone().scope())
}
