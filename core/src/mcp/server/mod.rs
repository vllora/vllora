pub mod service;
pub mod tools;

pub use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

use crate::mcp::server::tools::ListTracesRequest;
use crate::types::handlers::pagination::PaginatedResult;
use crate::types::metadata::services::trace::ListTracesQuery;
use crate::types::metadata::services::trace::TraceService;
use crate::types::traces::LangdbSpan;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content};
use rmcp::model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::Json;
use rmcp::{
    handler::server::router::tool::ToolRouter, tool, tool_handler, tool_router,
    ErrorData as McpError, ServerHandler,
};

#[derive(Clone)]
pub struct VlloraMcp<T: TraceService + Send + Sync + 'static> {
    /// Router for tool dispatch
    tool_router: ToolRouter<VlloraMcp<T>>,
    trace_service: T,
}

#[tool_router]
impl<T: TraceService + Send + Sync + 'static> VlloraMcp<T> {
    #[allow(dead_code)]
    pub fn new(trace_service: T) -> Self {
        Self {
            tool_router: Self::tool_router(),
            trace_service,
        }
    }

    #[tool(description = "Get Vllora version")]
    async fn get_version(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(env!(
            "CARGO_PKG_VERSION"
        ))]))
    }

    #[tool(name = "get_spans", description = "Get spans")]
    async fn get_traces(
        &self,
        Parameters(params): Parameters<ListTracesRequest>,
    ) -> Result<Json<PaginatedResult<LangdbSpan>>, String> {
        let range = params.get_range();
        let list_query = ListTracesQuery {
            project_slug: None,
            run_ids: params.run_ids.clone(),
            thread_ids: params.thread_ids.clone(),
            operation_names: params.operation_names.as_ref().map(|operation_names| {
                operation_names
                    .iter()
                    .map(|operation| operation.to_string())
                    .collect()
            }),
            parent_span_ids: params.parent_span_ids.clone(),
            start_time_min: range.map(|(start_time_min, _)| start_time_min),
            start_time_max: range.map(|(_, start_time_max)| start_time_max),
            limit: params.get_limit(),
            offset: params.get_offset(),
            ..Default::default()
        };
        Ok(Json(
            self.trace_service
                .list_paginated(list_query)
                .map_err(|e| e.to_string())?,
        ))
    }
}

#[tool_handler]
impl<T: TraceService + Send + Sync + 'static> ServerHandler for VlloraMcp<T> {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("This server provides a Vllora version tool that can get the current Vllora version.".to_string()),
        }
    }
}
