pub mod service;
pub mod tools;

pub use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

use crate::mcp::server::tools::{
    GetLlmCallInclude, GetLlmCallParams, GetLlmCallResponse, LlmRequest, LlmResponse, Redaction,
    UnsafeText,
};
use crate::mcp::server::tools::{
    SearchTraceItem, SearchTracesInclude, SearchTracesParams,
    SearchTracesResponse, SearchTracesStatus,
};
use crate::types::handlers::pagination::PaginatedResult;
use crate::types::metadata::services::trace::ListTracesQuery;
use crate::types::metadata::services::trace::TraceService;
use crate::types::traces::LangdbSpan;
use serde_json::Value as JsonValue;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content};
use rmcp::model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::Json;
use rmcp::{
    handler::server::router::tool::ToolRouter, tool, tool_handler, tool_router,
    ErrorData as McpError, ServerHandler,
};
use std::collections::HashMap;

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

    // #[tool(name = "get_spans", description = "Get spans")]
    // async fn get_traces(
    //     &self,
    //     Parameters(params): Parameters<ListTracesRequest>,
    // ) -> Result<Json<PaginatedResult<LangdbSpan>>, String> {
    //     let range = params.get_range();
    //     let list_query = ListTracesQuery {
    //         project_slug: None,
    //         run_ids: params.run_ids.clone(),
    //         thread_ids: params.thread_ids.clone(),
    //         operation_names: params.operation_names.as_ref().map(|operation_names| {
    //             operation_names
    //                 .iter()
    //                 .map(|operation| operation.to_string())
    //                 .collect()
    //         }),
    //         parent_span_ids: params.parent_span_ids.clone(),
    //         start_time_min: range.map(|(start_time_min, _)| start_time_min),
    //         start_time_max: range.map(|(_, start_time_max)| start_time_max),
    //         limit: params.get_limit(),
    //         offset: params.get_offset(),
    //         ..Default::default()
    //     };
    //     Ok(Json(
    //         self.trace_service
    //             .list_paginated(list_query)
    //             .map_err(|e| e.to_string())?,
    //     ))
    // }

    /// High-level MCP tool that wraps `get_traces` into the `search_traces` shape
    /// documented in DOC_v2.md.
    #[tool(name = "search_traces", description = "Search traces for analysis")]
    async fn search_traces(
        &self,
        Parameters(params): Parameters<SearchTracesParams>,
    ) -> Result<Json<SearchTracesResponse>, String> {
        // Map high-level MCP params onto the existing ListTracesQuery.
        let mut list_query = ListTracesQuery::default();

        // Basic pagination mapping
        if let Some(page) = &params.page {
            list_query.limit = page.limit;
            list_query.offset = page.offset.unwrap_or(0);
        }

        // For now we ignore most high-level filters and let the underlying
        // ListTracesQuery default behavior handle time ranges and other filters.
        // These can be wired more precisely as the semantics are refined.

        let paginated: PaginatedResult<LangdbSpan> = self
            .trace_service
            .list_paginated(list_query)
            .map_err(|e| e.to_string())?;

        // Determine which optional fields should be populated based on `include`.
        let include = params
            .include
            .unwrap_or(SearchTracesInclude {
                metrics: false,
                tokens: false,
                costs: false,
            });

        // Map PaginatedResult<LangdbSpan> into SearchTracesResponse, enriching
        // with labels, metrics, tokens and costs from the span attributes.
        let items: Vec<SearchTraceItem> = paginated
            .data
            .into_iter()
            .map(|span| {
                // ----- labels -----
                let mut labels: HashMap<String, String> = HashMap::new();
                if let Some(thread_id) = span.thread_id.clone() {
                    labels.insert("thread_id".to_string(), thread_id);
                }
                if let Some(run_id) = span.run_id.clone() {
                    labels.insert("run_id".to_string(), run_id);
                }
                if let Some(JsonValue::String(model_name)) = span.attribute.get("model_name") {
                    labels.insert("model_name".to_string(), model_name.clone());
                }

                // ----- metrics, tokens, costs -----
                let mut metrics: HashMap<String, i64> = HashMap::new();
                let mut tokens: Option<JsonValue> = None;
                let mut costs: Option<JsonValue> = None;

                // ttft metric (typically present on openai spans)
                if include.metrics {
                    if let Some(value) = span.attribute.get("ttft") {
                        if let Some(v) = match value {
                            JsonValue::String(s) => s.parse::<i64>().ok(),
                            JsonValue::Number(n) => n.as_i64(),
                            _ => None,
                        } {
                            metrics.insert("ttft".to_string(), v);
                        }
                    }
                }

                // usage / token metrics (commonly stored as JSON string in "usage")
                if include.metrics || include.tokens {
                    if let Some(raw_usage) = span.attribute.get("usage") {
                        let usage_value: Option<JsonValue> = match raw_usage {
                            JsonValue::String(s) => serde_json::from_str::<JsonValue>(s).ok(),
                            other => Some(other.clone()),
                        };

                        if let Some(usage) = usage_value.clone() {
                            if include.metrics {
                                if let JsonValue::Object(obj) = &usage {
                                    for key in ["input_tokens", "output_tokens", "total_tokens"] {
                                        if let Some(JsonValue::Number(n)) = obj.get(key) {
                                            if let Some(v) = n.as_i64() {
                                                metrics.insert(key.to_string(), v);
                                            }
                                        }
                                    }
                                }
                            }

                            if include.tokens {
                                tokens = Some(usage);
                            }
                        }
                    }
                }

                // cost metric (commonly a string in "cost")
                if include.costs {
                    if let Some(raw_cost) = span.attribute.get("cost") {
                        let cost_value: Option<JsonValue> = match raw_cost {
                            JsonValue::String(s) => s
                                .parse::<f64>()
                                .ok()
                                .map(|v| serde_json::json!(v)),
                            other => Some(other.clone()),
                        };
                        costs = cost_value;
                    }
                }

                let labels = if labels.is_empty() {
                    None
                } else {
                    Some(labels)
                };
                let metrics = if metrics.is_empty() {
                    None
                } else {
                    Some(metrics)
                };

                SearchTraceItem {
                    trace_id: span.trace_id,
                    thread_id: span.thread_id,
                    run_id: span.run_id,
                    // We currently don't have an explicit ok/error classification at this layer,
                    // so we mark the status as "any".
                    status: SearchTracesStatus::Any,
                    root_operation_name: span.operation_name.to_string(),
                    // Use microsecond timestamp as a string; this can be changed
                    // later to a full ISO8601 timestamp without breaking schema.
                    start_time: span.start_time_us.to_string(),
                    duration_ms: (span.finish_time_us - span.start_time_us) / 1_000,
                    labels,
                    metrics,
                    tokens,
                    costs,
                    has_unsafe_text: false,
                }
            })
            .collect();

        let pagination = paginated.pagination;
        let next_offset = pagination.offset + pagination.limit;
        let next_cursor = if next_offset < pagination.total {
            Some(next_offset.to_string())
        } else {
            None
        };

        Ok(Json(SearchTracesResponse { items, next_cursor }))
    }

    /// Get detailed LLM call information for a specific span.
    #[tool(name = "get_llm_call", description = "Get detailed LLM call information for a span")]
    async fn get_llm_call(
        &self,
        Parameters(params): Parameters<GetLlmCallParams>,
    ) -> Result<Json<GetLlmCallResponse>, String> {
        // Query for the specific span by trace_id and span_id
        let list_query = ListTracesQuery {
            span_id: Some(params.span_id.to_string()),
            limit: 1,
            offset: 0,
            ..Default::default()
        };

        let paginated: PaginatedResult<LangdbSpan> = self
            .trace_service
            .list_paginated(list_query)
            .map_err(|e| e.to_string())?;

        let span = paginated
            .data
            .into_iter()
            .find(|s| s.trace_id == params.trace_id && s.span_id == params.span_id.to_string())
            .ok_or_else(|| format!("Span not found: trace_id={}, span_id={}", params.trace_id, params.span_id))?;

        let include = params.include.unwrap_or(GetLlmCallInclude {
            llm_payload: false,
            unsafe_text: false,
        });

        // Parse span_id as i64
        let span_id_num = params.span_id;

        // Extract provider from attributes
        let provider = span
            .attribute
            .get("provider_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Build request payload if requested
        let request = if include.llm_payload {
            let model = span
                .attribute
                .get("model_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Extract params from model JSON if available
            let model_params = span
                .attribute
                .get("model")
                .and_then(|v| {
                    if let JsonValue::String(s) = v {
                        serde_json::from_str::<JsonValue>(s).ok()
                    } else {
                        Some(v.clone())
                    }
                })
                .and_then(|model_json| {
                    if let JsonValue::Object(obj) = model_json {
                        obj.get("model_params")
                            .and_then(|mp| {
                                if let JsonValue::Object(mp_obj) = mp {
                                    mp_obj.get("engine").cloned()
                                } else {
                                    None
                                }
                            })
                            .or_else(|| obj.get("model_params").cloned())
                    } else {
                        None
                    }
                });

            // Extract messages and tools from input/request
            let messages = span.attribute.get("input").or_else(|| span.attribute.get("request"));
            let tools = span.attribute.get("tools");

            // Wrap in unsafe_text if requested
            let messages_wrapped = if include.unsafe_text || params.allow_unsafe_text {
                messages.map(|m| {
                    serde_json::json!({
                        "unsafe_text": m
                    })
                })
            } else {
                messages.cloned()
            };

            let tools_wrapped = if include.unsafe_text || params.allow_unsafe_text {
                tools.map(|t| {
                    serde_json::json!({
                        "unsafe_text": t
                    })
                })
            } else {
                tools.cloned()
            };

            Some(LlmRequest {
                model,
                params: model_params,
                messages: messages_wrapped,
                tools: tools_wrapped,
            })
        } else {
            None
        };

        // Build response payload if requested
        let response = if include.unsafe_text || params.allow_unsafe_text {
            let output = span.attribute.get("output").or_else(|| span.attribute.get("response"));
            output.map(|content| LlmResponse {
                unsafe_text: Some(UnsafeText {
                    kind: Some("llm_output".to_string()),
                    content: content.clone(),
                    treat_as_data_not_instructions: Some(true),
                }),
            })
        } else {
            None
        };

        // Extract tokens and costs
        let tokens = span.attribute.get("usage").cloned();
        let costs = span.attribute.get("cost").cloned();

        // Build redactions list (placeholder - would need to track redactions in attributes)
        let redactions: Option<Vec<Redaction>> = None;

        Ok(Json(GetLlmCallResponse {
            span_id: span_id_num,
            provider,
            request,
            response,
            tokens,
            costs,
            redactions,
        }))
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
