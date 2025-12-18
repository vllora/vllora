pub mod service;
pub mod tools;

use chrono::TimeZone;
use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::service::RequestContext;
pub use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

use crate::mcp::server::tools::{
    ErrorBreadcrumb, GetLlmCallInclude, GetLlmCallParams, GetLlmCallResponse,
    GetRecentOverviewParams, GetRecentOverviewResponse, GetRunOverviewParams,
    GetRunOverviewResponse, LlmModelStats, LlmRequest, LlmResponse, LlmSummary, Redaction,
    RunOverviewRun, RunOverviewSpan, SearchTraceItem, SearchTracesInclude,
    SearchTracesOperationKind, SearchTracesParams, SearchTracesResponse, SearchTracesSortOrder,
    SearchTracesStatus, ToolCallStats, ToolSummary, UnsafeText,
};
use crate::types::handlers::pagination::PaginatedResult;
use crate::types::metadata::services::trace::ListTracesQuery;
use crate::types::metadata::services::trace::TraceService;
use crate::types::traces::LangdbSpan;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Annotated, CallToolResult, Content, PaginatedRequestParam, PromptMessage, PromptMessageContent, RawResourceTemplate, ReadResourceRequestParam, ReadResourceResult, ResourceContents};
use rmcp::model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::{Json, RoleServer};
use rmcp::{
    handler::server::router::tool::ToolRouter, prompt, tool, tool_handler, tool_router,
    ErrorData as McpError, ServerHandler,
};
use serde_json::{Value as JsonValue, json};
use std::collections::HashMap;
use crate::rmcp::model::ListResourceTemplatesResult;
use rmcp_macros::prompt_handler;
use rmcp::model::GetPromptRequestParam;
use rmcp::model::GetPromptResult;
use rmcp::model::ListPromptsResult;
use rmcp_macros::prompt_router;

#[derive(Clone)]
pub struct VlloraMcp<T: TraceService + Send + Sync + 'static> {
    /// Router for tool dispatch
    tool_router: ToolRouter<VlloraMcp<T>>,
    prompt_router: PromptRouter<VlloraMcp<T>>,
    trace_service: T,
}

#[tool_router]
#[prompt_router]
impl<T: TraceService + Send + Sync + 'static> VlloraMcp<T> {
    #[allow(dead_code)]
    pub fn new(trace_service: T) -> Self {
        Self {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
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
        let mut list_query: ListTracesQuery = ListTracesQuery::default();
        // Basic pagination mapping
        if let Some(page) = &params.page {
            list_query.limit = page.limit;
            list_query.offset = page.offset.unwrap_or(0);
        }

        // Apply filters
        if let Some(filters) = &params.filters {
            if let Some(run_id) = &filters.run_id {
                list_query.run_ids = Some(vec![run_id.clone()]);
            }
            if let Some(thread_id) = &filters.thread_id {
                list_query.thread_ids = Some(vec![thread_id.clone()]);
            }
            if let Some(_model) = &filters.model {
                // Model name is stored in attributes, so we can't filter directly
                // This would require a more complex query or post-filtering
            }
            if let Some(operation_name) = &filters.operation_name {
                let op_str = match operation_name {
                    SearchTracesOperationKind::Run => "run",
                    SearchTracesOperationKind::Agent => "agent",
                    SearchTracesOperationKind::Task => "task",
                    SearchTracesOperationKind::Tools | SearchTracesOperationKind::ToolCall => {
                        "tools"
                    }
                    SearchTracesOperationKind::Openai => "openai",
                    SearchTracesOperationKind::Anthropic => "anthropic",
                    SearchTracesOperationKind::Bedrock => "bedrock",
                    SearchTracesOperationKind::Gemini => "gemini",
                    SearchTracesOperationKind::CloudApiInvoke => "cloud_api_invoke",
                    SearchTracesOperationKind::ApiInvoke => "api_invoke",
                    SearchTracesOperationKind::ModelCall | SearchTracesOperationKind::LlmCall => {
                        "model_call"
                    }
                };
                list_query.operation_names = Some(vec![op_str.to_string()]);
            }
            if let Some(text) = &filters.text {
                list_query.text_search = Some(text.clone());
            }
            if let Some(true) = filters.has_thread {
                list_query.filter_not_null_thread = true;
            }
            if let Some(true) = filters.has_run {
                list_query.filter_not_null_run = true;
            }
        }

        // Apply time range filters
        if let Some(time_range) = &params.time_range {
            if let Some(last_n_minutes) = time_range.last_n_minutes {
                let now = chrono::Utc::now().timestamp_micros();
                let start_time_min = now - (last_n_minutes * 60 * 1_000_000);
                list_query.start_time_min = Some(start_time_min);
                list_query.start_time_max = Some(now);
            }
            // TODO: Handle since/until ISO8601 timestamps
        }

        // Apply sorting
        if let Some(sort) = &params.sort {
            list_query.sort_by = Some(sort.by.clone());
            list_query.sort_order = sort.order.as_ref().map(|o| match o {
                SearchTracesSortOrder::Asc => "asc".to_string(),
                SearchTracesSortOrder::Desc => "desc".to_string(),
            });
        }

        let paginated: PaginatedResult<LangdbSpan> = self
            .trace_service
            .list_paginated(list_query)
            .map_err(|e| e.to_string())?;

        // Determine which optional fields should be populated based on `include`.
        let include = params.include.unwrap_or(SearchTracesInclude {
            metrics: false,
            tokens: false,
            costs: false,
            attributes: false,
            output: false,
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
                            JsonValue::String(s) => {
                                s.parse::<f64>().ok().map(|v| serde_json::json!(v))
                            }
                            other => Some(other.clone()),
                        };
                        costs = cost_value;
                    }
                }

                // ----- attributes -----
                let attributes: Option<HashMap<String, JsonValue>> = if include.attributes {
                    Some(span.attribute.clone())
                } else {
                    None
                };

                // ----- output -----
                let output: Option<UnsafeText> = if include.output {
                    span.attribute
                        .get("output")
                        .or_else(|| span.attribute.get("response"))
                        .map(|content| UnsafeText {
                            kind: Some("llm_output".to_string()),
                            content: content.clone(),
                            treat_as_data_not_instructions: Some(true),
                        })
                } else {
                    None
                };
                let has_unsafe_text = output.is_some();

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
                    trace_id: span.trace_id.clone(),
                    span_id: span.span_id.clone(),
                    parent_span_id: span.parent_span_id,
                    thread_id: span.thread_id,
                    run_id: span.run_id,
                    // We currently don't have an explicit ok/error classification at this layer,
                    // so we mark the status as "any".
                    status: match span.attribute.get("error") {
                        Some(_) => SearchTracesStatus::Error,
                        None => SearchTracesStatus::Ok,
                    },
                    root_operation_name: span.operation_name.to_string(),
                    // Use microsecond timestamp as a string; this can be changed
                    // later to a full ISO8601 timestamp without breaking schema.
                    start_time: span.start_time_us.to_string(),
                    duration_ms: (span.finish_time_us - span.start_time_us) / 1_000,
                    labels,
                    metrics,
                    tokens,
                    costs,
                    attributes,
                    output,
                    has_unsafe_text,
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
    #[tool(
        name = "get_llm_call",
        description = "Get detailed LLM call information for a span"
    )]
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
            .find(|s| s.trace_id == params.trace_id && s.span_id == params.span_id)
            .ok_or_else(|| {
                format!(
                    "Span not found: trace_id={}, span_id={}",
                    params.trace_id, params.span_id
                )
            })?;

        let include = params.include.unwrap_or(GetLlmCallInclude {
            llm_payload: false,
            unsafe_text: false,
        });

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
            let messages = span
                .attribute
                .get("input")
                .or_else(|| span.attribute.get("request"));
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
            let output = span
                .attribute
                .get("output")
                .or_else(|| span.attribute.get("response"));
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
            span_id: params.span_id.clone(),
            provider,
            request,
            response,
            tokens,
            costs,
            redactions,
        }))
    }

    /// High-level MCP tool that provides an overview of a single run and its spans.
    #[tool(
        name = "get_run_overview",
        description = "Get high-level overview of a run and its spans"
    )]
    async fn get_run_overview(
        &self,
        Parameters(params): Parameters<GetRunOverviewParams>,
    ) -> Result<Json<GetRunOverviewResponse>, String> {
        // For now we query spans for this run (up to a reasonable limit).
        let list_query = ListTracesQuery {
            project_slug: None,
            span_id: None,
            run_ids: Some(vec![params.run_id.clone()]),
            thread_ids: None,
            operation_names: None,
            parent_span_ids: None,
            filter_null_thread: false,
            filter_null_run: false,
            filter_null_operation: false,
            filter_null_parent: false,
            filter_not_null_thread: false,
            filter_not_null_run: false,
            filter_not_null_operation: false,
            filter_not_null_parent: false,
            start_time_min: None,
            start_time_max: None,
            // A reasonable default page size for an overview; can be expanded later if needed
            limit: 100,
            offset: 0,
            text_search: None,
            sort_by: None,
            sort_order: None,
        };

        let paginated: PaginatedResult<LangdbSpan> = self
            .trace_service
            .list_paginated(list_query)
            .map_err(|e| e.to_string())?;

        if paginated.data.is_empty() {
            return Err(format!("No spans found for run_id={}", params.run_id));
        }

        // Sort spans by start time to get a stable ordering
        let mut spans = paginated.data;
        spans.sort_by_key(|s| s.start_time_us);

        // Compute basic run-level timing
        let start_time_us = spans.iter().map(|s| s.start_time_us).min().unwrap_or(0);
        let finish_time_us = spans
            .iter()
            .map(|s| s.finish_time_us)
            .max()
            .unwrap_or(start_time_us);
        let duration_ms = (finish_time_us - start_time_us) / 1_000;

        // Convert microseconds to ISO8601 (UTC)
        let start_time = {
            let secs = start_time_us / 1_000_000;
            let micros = (start_time_us % 1_000_000) as u32;
            let dt = chrono::Utc
                .timestamp_opt(secs, micros * 1_000)
                .single()
                .ok_or_else(|| "Failed to convert start_time_us to datetime".to_string())?;
            dt.to_rfc3339()
        };

        // Determine root span: prefer an explicit "run" operation, otherwise first span.
        let root_span = spans
            .iter()
            .find(|s| matches!(s.operation_name, crate::types::traces::Operation::Run))
            .unwrap_or(&spans[0]);
        let root_span_id = root_span.span_id.clone();

        // Derive run-level labels (e.g. agent) from the root span attributes.
        let mut run_labels = HashMap::new();
        if let Some(agent) = root_span.attribute.get("agent").and_then(|v| v.as_str()) {
            run_labels.insert("agent".to_string(), agent.to_string());
        }
        let run_label = if run_labels.is_empty() {
            None
        } else {
            Some(run_labels)
        };

        // Derive span-level statuses and collect data for later summaries.
        // Heuristic:
        // - If a span has an "error" attribute, we mark it as "error".
        // - Otherwise we mark it as "ok".
        // - If no spans are "error", the run is "ok"; otherwise "error".
        let mut has_error_span = false;
        let span_statuses: Vec<String> = spans
            .iter()
            .map(|s| {
                if s.attribute.contains_key("error") {
                    has_error_span = true;
                    "error".to_string()
                } else {
                    "ok".to_string()
                }
            })
            .collect();

        let run_status = if has_error_span {
            "error".to_string()
        } else {
            "ok".to_string()
        };

        let run_overview = RunOverviewRun {
            run_id: params.run_id.clone(),
            status: run_status,
            start_time,
            duration_ms,
            label: run_label,
            root_span_id: root_span_id.clone(),
        };

        // Build span tree entries and derive "kind" per span.
        let span_tree: Vec<RunOverviewSpan> = spans
            .iter()
            .zip(span_statuses.iter())
            .map(|(s, status)| {
                let kind = match &s.operation_name {
                    crate::types::traces::Operation::Openai
                    | crate::types::traces::Operation::Anthropic
                    | crate::types::traces::Operation::Bedrock
                    | crate::types::traces::Operation::Gemini
                    | crate::types::traces::Operation::ModelCall => "llm".to_string(),
                    crate::types::traces::Operation::Tools => "tool".to_string(),
                    _ => "internal".to_string(),
                };

                RunOverviewSpan {
                    span_id: s.span_id.clone(),
                    parent_span_id: s.parent_span_id.clone(),
                    operation_name: s.operation_name.to_string(),
                    kind,
                    status: status.clone(),
                }
            })
            .collect();

        // Derive agents_used from any span that carries an "agent" attribute.
        let mut agent_set: std::collections::HashSet<String> = std::collections::HashSet::new();
        for span in &spans {
            if let Some(agent) = span.attribute.get("agent").and_then(|v| v.as_str()) {
                agent_set.insert(agent.to_string());
            }
        }
        let agents_used: Vec<String> = agent_set.into_iter().collect();

        // Error breadcrumbs: one breadcrumb per span that has an "error" attribute.
        let mut error_breadcrumbs: Vec<ErrorBreadcrumb> = Vec::new();
        for (s, status) in spans.iter().zip(span_statuses.iter()) {
            if status == "error" {
                // Raw error string if present on the span.
                let error = s
                    .attribute
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                error_breadcrumbs.push(ErrorBreadcrumb {
                    span_id: s.span_id.clone(),
                    operation_name: s.operation_name.to_string(),
                    error,
                    error_payload: s.attribute.get("error_payload").cloned(),
                });
            }
        }

        // LLM summaries: for spans classified as "llm", use attributes to extract provider/model.
        let mut llm_summaries: Vec<LlmSummary> = Vec::new();
        for s in &spans {
            let is_llm = matches!(
                s.operation_name,
                crate::types::traces::Operation::Openai
                    | crate::types::traces::Operation::Anthropic
                    | crate::types::traces::Operation::Bedrock
                    | crate::types::traces::Operation::Gemini
                    | crate::types::traces::Operation::ModelCall
            );
            if !is_llm {
                continue;
            }

            // Provider/model are stored as attributes on the span, when present.
            let provider = s
                .attribute
                .get("provider_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let model = s
                .attribute
                .get("model_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Approximate message_count and tool_count from attributes if they exist.
            // We look for "input" and "tools" attributes, which may contain JSON.
            let mut message_count: i64 = 0;
            if let Some(input_val) = s.attribute.get("input") {
                // If stored as a string containing JSON, try to parse it.
                let parsed = if let Some(s) = input_val.as_str() {
                    serde_json::from_str::<JsonValue>(s).ok()
                } else {
                    Some(input_val.clone())
                };

                if let Some(v) = parsed {
                    if let JsonValue::Array(arr) = &v {
                        message_count = arr.len() as i64;
                    } else if let JsonValue::Object(obj) = &v {
                        if let Some(JsonValue::Array(msgs)) = obj.get("messages") {
                            message_count = msgs.len() as i64;
                        }
                    }
                }
            }

            let mut tool_count: i64 = 0;
            if let Some(tools_val) = s.attribute.get("tools") {
                let parsed = if let Some(s) = tools_val.as_str() {
                    serde_json::from_str::<JsonValue>(s).ok()
                } else {
                    Some(tools_val.clone())
                };

                if let Some(JsonValue::Array(arr)) = &parsed {
                    tool_count = arr.len() as i64;
                }
            }

            llm_summaries.push(LlmSummary {
                span_id: s.span_id.clone(),
                provider,
                model,
                message_count,
                tool_count,
            });
        }

        // Tool summaries: for spans classified as "tool", derive tool name and hashes from attributes.
        let mut tool_summaries: Vec<ToolSummary> = Vec::new();
        for (s, status) in spans.iter().zip(span_statuses.iter()) {
            let is_tool = matches!(s.operation_name, crate::types::traces::Operation::Tools);
            if !is_tool {
                continue;
            }

            let tool_name = s
                .attribute
                .get("tool_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let args_sha256 = s
                .attribute
                .get("tool_args_sha256")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let result_sha256 = s
                .attribute
                .get("tool_result_sha256")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            tool_summaries.push(ToolSummary {
                span_id: s.span_id.clone(),
                tool_name,
                args_sha256,
                result_sha256,
                status: status.clone(),
            });
        }

        Ok(Json(GetRunOverviewResponse {
            run: run_overview,
            span_tree,
            agents_used,
            error_breadcrumbs,
            llm_summaries,
            tool_summaries,
        }))
    }

    /// High-level MCP tool that provides an overview of recent LLM and tool activity
    /// for the last N minutes.
    #[tool(
        name = "get_recent_stats",
        description = "Get aggregated overview of recent LLM and tool calls for the last N minutes"
    )]
    async fn get_recent_stats(
        &self,
        Parameters(params): Parameters<GetRecentOverviewParams>,
    ) -> Result<Json<GetRecentOverviewResponse>, String> {
        if params.last_n_minutes <= 0 {
            return Err("last_n_minutes must be > 0".to_string());
        }

        // Compute time window in microseconds.
        let now_us = chrono::Utc::now().timestamp_micros();
        let window_us = params.last_n_minutes * 60 * 1_000_000;
        let start_us = now_us.saturating_sub(window_us);

        // Helper to convert microseconds since epoch to RFC3339.
        fn micros_to_rfc3339(ts_us: i64) -> Result<String, String> {
            let secs = ts_us / 1_000_000;
            let micros = (ts_us % 1_000_000) as u32;
            chrono::Utc
                .timestamp_opt(secs, micros * 1_000)
                .single()
                .ok_or_else(|| "Failed to convert timestamp to datetime".to_string())
                .map(|dt| dt.to_rfc3339())
        }

        let window_start = micros_to_rfc3339(start_us)?;
        let window_end = micros_to_rfc3339(now_us)?;

        // Aggregate LLM calls grouped by model.
        let mut llm_stats_map: HashMap<String, (i64, i64)> = HashMap::new(); // model -> (ok, error)

        // Operations considered LLM calls.
        let llm_operations: &[&str] = &["model_call"];

        let limit: i64 = 1000;
        let mut offset: i64 = 0;

        loop {
            let list_query = ListTracesQuery {
                project_slug: None,
                span_id: None,
                run_ids: None,
                thread_ids: None,
                operation_names: Some(llm_operations.iter().map(|s| s.to_string()).collect()),
                parent_span_ids: None,
                filter_null_thread: false,
                filter_null_run: false,
                filter_null_operation: false,
                filter_null_parent: false,
                filter_not_null_thread: false,
                filter_not_null_run: false,
                filter_not_null_operation: false,
                filter_not_null_parent: false,
                start_time_min: Some(start_us),
                start_time_max: Some(now_us),
                limit,
                offset,
                text_search: None,
                sort_by: Some("start_time".to_string()),
                sort_order: Some("desc".to_string()),
            };

            let page: PaginatedResult<LangdbSpan> = self
                .trace_service
                .list_paginated(list_query)
                .map_err(|e| e.to_string())?;

            for span in &page.data {
                let model = span
                    .attribute
                    .get("model_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let is_error = span.attribute.contains_key("error");
                let entry = llm_stats_map.entry(model).or_insert((0, 0));
                if is_error {
                    entry.1 += 1;
                } else {
                    entry.0 += 1;
                }
            }

            let pagination = page.pagination;
            let next_offset = pagination.offset + pagination.limit;
            if next_offset >= pagination.total {
                break;
            }
            offset = next_offset;
        }

        let llm_calls: Vec<LlmModelStats> = llm_stats_map
            .into_iter()
            .map(|(model, (ok_count, error_count))| LlmModelStats {
                model,
                ok_count,
                error_count,
                total_count: ok_count + error_count,
            })
            .collect();

        // Aggregate tool calls grouped by tool_name.
        let mut tool_stats_map: HashMap<String, (i64, i64)> = HashMap::new(); // tool_name -> (ok, error)
        let mut offset_tools: i64 = 0;

        loop {
            let list_query = ListTracesQuery {
                project_slug: None,
                span_id: None,
                run_ids: None,
                thread_ids: None,
                operation_names: Some(vec!["tools".to_string()]),
                parent_span_ids: None,
                filter_null_thread: false,
                filter_null_run: false,
                filter_null_operation: false,
                filter_null_parent: false,
                filter_not_null_thread: false,
                filter_not_null_run: false,
                filter_not_null_operation: false,
                filter_not_null_parent: false,
                start_time_min: Some(start_us),
                start_time_max: Some(now_us),
                limit,
                offset: offset_tools,
                text_search: None,
                sort_by: Some("start_time".to_string()),
                sort_order: Some("desc".to_string()),
            };

            let page: PaginatedResult<LangdbSpan> = self
                .trace_service
                .list_paginated(list_query)
                .map_err(|e| e.to_string())?;

            for span in &page.data {
                let tool_name = span
                    .attribute
                    .get("tool.name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let is_error = span.attribute.contains_key("error");
                let entry = tool_stats_map.entry(tool_name).or_insert((0, 0));
                if is_error {
                    entry.1 += 1;
                } else {
                    entry.0 += 1;
                }
            }

            let pagination = page.pagination;
            let next_offset = pagination.offset + pagination.limit;
            if next_offset >= pagination.total {
                break;
            }
            offset_tools = next_offset;
        }

        let tool_calls: Vec<ToolCallStats> = tool_stats_map
            .into_iter()
            .map(|(tool_name, (ok_count, error_count))| ToolCallStats {
                tool_name,
                ok_count,
                error_count,
                total_count: ok_count + error_count,
            })
            .collect();

        Ok(Json(GetRecentOverviewResponse {
            window_minutes: params.last_n_minutes,
            window_start,
            window_end,
            llm_calls,
            tool_calls,
        }))
    }

    /// Prompt for debugging errors in LLM traces
    #[prompt(
        name = "debug_errors",
        description = "Guide for debugging errors in LLM traces and runs"
    )]
    async fn debug_errors_prompt(
        &self,
        Parameters(_args): Parameters<HashMap<String, String>>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: rmcp::model::PromptMessageRole::User,
                content: PromptMessageContent::text(
                    r#"You are helping debug errors in LLM traces. Follow this systematic approach:

1. **Find Recent Errors**: Use `search_traces` with:
   - `time_range: { last_n_minutes: 30 }` (or appropriate time window)
   - `filters: { status: "error" }`
   - `include: { metrics: true, tokens: true, costs: true }` to get context

2. **Analyze Error Context**: For each error trace:
   - Note the `trace_id`, `span_id`, `run_id`, and `thread_id`
   - Check the `duration_ms` and `metrics` (like `ttft` - time to first token)
   - Review `labels` for model_name, thread_id, run_id

3. **Get Run Overview**: If a `run_id` is available, use `get_run_overview` to:
   - See the complete span tree and relationships
   - Identify error breadcrumbs showing where failures occurred
   - Understand the flow: LLM calls → tool calls → responses

4. **Inspect Specific LLM Call**: Use `get_llm_call` with:
   - `trace_id` and `span_id` from the error trace
   - `include: { llm_payload: true, unsafe_text: true }` to see full request/response
   - Check `tokens` and `costs` for anomalies

5. **Check System Health**: Use `get_recent_stats` to:
   - See error rates by model (`llm_calls` with `error_count`)
   - Identify problematic tools (`tool_calls` with `error_count`)
   - Compare error rates across time windows

**Common Error Patterns to Look For**:
- High `ttft` (time to first token) suggests slow model responses
- Token limit errors in `tokens` field
- Tool execution failures in `tool_summaries`
- Authentication errors in error breadcrumbs
- Cost anomalies suggesting billing issues

**Next Steps After Finding Errors**:
- Trace the error through the span tree using `get_run_overview`
- Compare failed calls with successful ones using `search_traces`
- Check if errors are model-specific or tool-specific using `get_recent_stats`
"#
                    .to_string(),
                ),
            },
        ])
    }

    /// Prompt for analyzing performance issues
    #[prompt(
        name = "analyze_performance",
        description = "Guide for analyzing performance issues in LLM traces"
    )]
    async fn analyze_performance_prompt(
        &self,
        Parameters(_args): Parameters<HashMap<String, String>>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: rmcp::model::PromptMessageRole::User,
                content: PromptMessageContent::text(
                    r#"You are helping analyze performance issues in LLM traces. Use this approach:

1. **Identify Slow Traces**: Use `search_traces` with:
   - `include: { metrics: true }` to get `ttft` and duration data
   - Sort by `duration_ms` descending to find slowest traces
   - Filter by `operation_name: "llm_call"` to focus on LLM performance

2. **Analyze Metrics**:
   - **TTFT (Time to First Token)**: High values indicate slow model responses
   - **Duration**: Total time including all processing
   - **Token Usage**: High `input_tokens` or `output_tokens` can slow responses
   - Compare metrics across different models using `get_recent_stats`

3. **Check Token Efficiency**: Use `get_llm_call` to:
   - Review `tokens` field for input/output ratios
   - Check if messages are unnecessarily long
   - Identify if tool definitions are bloating requests

4. **Analyze Run Performance**: Use `get_run_overview` to:
   - See total `duration_ms` for the run
   - Identify bottlenecks in the span tree
   - Check if tool calls are slowing down the flow
   - Look for sequential operations that could be parallelized

5. **Compare Performance**: Use `search_traces` to:
   - Compare same model across different time periods
   - Compare different models for the same task
   - Identify performance regressions over time

**Performance Red Flags**:
- TTFT > 5 seconds for most models
- Duration > 30 seconds for simple queries
- High token counts without corresponding value
- Many sequential tool calls in span tree
- Repeated LLM calls that could be cached

**Optimization Opportunities**:
- Reduce message history length
- Simplify tool definitions
- Cache frequent LLM responses
- Parallelize independent tool calls
- Use faster models for simple tasks
"#
                    .to_string(),
                ),
            },
        ])
    }

    /// Prompt for understanding run flows
    #[prompt(
        name = "understand_run_flow",
        description = "Guide for understanding and analyzing complete agent runs"
    )]
    async fn understand_run_flow_prompt(
        &self,
        Parameters(_args): Parameters<HashMap<String, String>>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: rmcp::model::PromptMessageRole::User,
                content: PromptMessageContent::text(
                    r#"You are helping understand agent run flows. Use this systematic approach:

1. **Get Run Overview**: Start with `get_run_overview` using a `run_id`:
   - Review the `run` object for status, duration, and labels
   - Examine the `span_tree` to understand the hierarchy
   - Check `error_breadcrumbs` for any failures
   - Review `llm_summaries` and `tool_summaries` for quick insights

2. **Understand Span Relationships**:
   - The `span_tree` shows parent-child relationships
   - Root span (no `parent_span_id`) is typically the run entry point
   - LLM spans (`kind: "llm"`) call models
   - Tool spans (`kind: "tool"`) execute tools
   - Internal spans (`kind: "internal"`) are framework operations

3. **Trace the Flow**:
   - Start from the root span
   - Follow `parent_span_id` → `span_id` relationships
   - Identify decision points where LLM calls lead to tool calls
   - Note where errors occur in the flow

4. **Analyze LLM Interactions**: For each LLM span:
   - Use `get_llm_call` with the `span_id` to see:
     - Full request messages and context
     - Tool definitions used
     - Response content
     - Token usage and costs

5. **Examine Tool Usage**: For tool spans:
   - Check `tool_summaries` for tool names
   - Review `args_sha256` and `result_sha256` for consistency
   - Identify which tools are called most frequently
   - Check tool error rates

6. **Find Related Traces**: Use `search_traces` with:
   - `filters: { run_id: "..." }` to see all traces in the run
   - `filters: { thread_id: "..." }` to see conversation context
   - Sort by `start_time` ascending to see chronological flow

**Key Questions to Answer**:
- What was the agent trying to accomplish? (Check root span and labels)
- How many LLM calls were made? (Count `llm_summaries`)
- What tools were used? (Review `tool_summaries`)
- Where did it fail? (Check `error_breadcrumbs`)
- How long did it take? (Check `duration_ms`)
- What was the cost? (Sum costs from individual LLM calls)

**Common Patterns**:
- **Agent Loop**: LLM → Tool → LLM → Tool (repeated)
- **Parallel Tools**: Multiple tool spans with same parent
- **Error Recovery**: Error breadcrumb followed by retry spans
- **Cost Spikes**: Many LLM calls with high token counts
"#
                    .to_string(),
                ),
            },
        ])
    }

    /// Prompt for effective trace searching
    #[prompt(
        name = "search_traces_guide",
        description = "Best practices guide for searching and filtering traces effectively"
    )]
    async fn search_traces_guide_prompt(
        &self,
        Parameters(_args): Parameters<HashMap<String, String>>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: rmcp::model::PromptMessageRole::User,
                content: PromptMessageContent::text(
                    r#"You are helping search traces effectively. Follow these best practices:

**1. Start with Time Ranges**:
- Use `time_range: { last_n_minutes: 30 }` for recent debugging
- Use `since` and `until` ISO8601 timestamps for precise windows
- Start narrow, then expand if needed

**2. Use Filters Strategically**:
- `status: "error"` - Find failures quickly
- `operation_name: "llm_call"` or `"tool_call"` - Focus on specific operations
- `model: "gpt-4o-mini"` - Filter by specific model
- `thread_id` or `run_id` - Find related traces
- `text: "search term"` - Search content (messages, responses, errors)
- `has_thread: true` or `has_run: true` - Find structured traces

**3. Optimize Include Flags**:
- **Quick searches**: Minimal or no `include` flags
- **Debugging**: `{ metrics: true, tokens: true, costs: true }`
- **Deep inspection**: `{ attributes: true, output: true }` (can be large!)

**4. Use Pagination**:
- Start with `limit: 20-50` for manageable results
- Use `next_cursor` from response for next page
- Increase limit if you need more context

**5. Sort Effectively**:
- `sort: { by: "start_time", order: "desc" }` - Most recent first
- `sort: { by: "duration_ms", order: "desc" }` - Slowest first
- `sort: { by: "start_time", order: "asc" }` - Chronological flow

**Common Search Patterns**:

**Find Recent Errors**:
```json
{
  "time_range": { "last_n_minutes": 60 },
  "filters": { "status": "error" },
  "include": { "metrics": true, "tokens": true }
}
```

**Find Traces by Model**:
```json
{
  "time_range": { "last_n_minutes": 30 },
  "filters": { "model": "gpt-4o-mini" },
  "include": { "costs": true, "tokens": true }
}
```

**Search Content**:
```json
{
  "filters": { "text": "authentication failed" },
  "page": { "limit": 20 }
}
```

**Find Thread Traces**:
```json
{
  "filters": { "thread_id": "thread-123" },
  "sort": { "by": "start_time", "order": "asc" }
}
```

**Performance Analysis**:
```json
{
  "time_range": { "last_n_minutes": 60 },
  "filters": { "operation_name": "llm_call" },
  "include": { "metrics": true },
  "sort": { "by": "duration_ms", "order": "desc" }
}
```

**Tips**:
- Combine filters for precise results
- Use text search when you know specific error messages
- Check `labels` in results for additional filtering context
- Use `get_llm_call` for detailed inspection after finding traces
- Use `get_run_overview` to understand trace relationships
"#
                    .to_string(),
                ),
            },
        ])
    }

    /// Prompt for monitoring system health
    #[prompt(
        name = "monitor_system_health",
        description = "Guide for monitoring LLM system health and usage"
    )]
    async fn monitor_system_health_prompt(
        &self,
        Parameters(_args): Parameters<HashMap<String, String>>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: rmcp::model::PromptMessageRole::User,
                content: PromptMessageContent::text(
                    r#"You are helping monitor system health. Use this approach:

**1. Get Recent Statistics**: Use `get_recent_stats` with appropriate time windows:
   - `last_n_minutes: 15` - Current activity
   - `last_n_minutes: 60` - Hourly trends
   - `last_n_minutes: 1440` - Daily patterns

**2. Analyze LLM Health** (`llm_calls` array):
   - **Error Rate**: `error_count / total_count` per model
   - **Success Rate**: `ok_count / total_count` per model
   - **Model Usage**: `total_count` shows which models are used most
   - **Compare Models**: Look for models with unusually high error rates

**3. Analyze Tool Health** (`tool_calls` array):
   - **Tool Reliability**: `ok_count / total_count` per tool
   - **Problematic Tools**: Tools with high `error_count`
   - **Tool Usage**: Most frequently used tools
   - **Tool Trends**: Compare error rates over time

**4. Deep Dive into Issues**:
   - If a model has high error rate, use `search_traces` with:
     - `filters: { model: "...", status: "error" }`
     - `include: { metrics: true, tokens: true }`
   - If a tool has high error rate, use `search_traces` with:
     - `filters: { operation_name: "tool_call", status: "error" }`
     - Check error breadcrumbs in `get_run_overview`

**5. Monitor Costs**: Use `search_traces` with:
   - `include: { costs: true, tokens: true }`
   - Filter by model to compare costs
   - Sort by costs to find expensive operations
   - Use `get_llm_call` for detailed cost breakdown

**6. Track Performance Trends**:
   - Compare `get_recent_stats` across different time windows
   - Look for increasing error rates
   - Monitor token usage trends
   - Track cost trends over time

**Health Indicators**:

**Good Health**:
- Error rates < 5% for models
- Error rates < 10% for tools
- Consistent response times
- Stable token usage patterns

**Warning Signs**:
- Error rates > 10% for models
- Error rates > 20% for tools
- Spiking response times
- Unusual token usage spikes
- Cost anomalies

**Critical Issues**:
- Error rates > 25%
- Complete model failures
- Tool failures blocking workflows
- Cost spikes indicating issues

**Regular Monitoring Checklist**:
1. Check `get_recent_stats` for overall health
2. Review error rates by model and tool
3. Identify any new error patterns
4. Check cost trends
5. Review performance metrics
6. Investigate any anomalies with `search_traces`
"#
                    .to_string(),
                ),
            },
        ])
    }

    /// Prompt for cost analysis
    #[prompt(
        name = "analyze_costs",
        description = "Guide for analyzing costs and token usage in LLM traces"
    )]
    async fn analyze_costs_prompt(
        &self,
        Parameters(_args): Parameters<HashMap<String, String>>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: rmcp::model::PromptMessageRole::User,
                content: PromptMessageContent::text(
                    r#"You are helping analyze costs and token usage. Follow this approach:

**1. Get Cost Overview**: Use `search_traces` with:
   - `include: { costs: true, tokens: true }`
   - Filter by time range to analyze specific periods
   - Filter by model to compare model costs
   - Sort by costs to find most expensive operations

**2. Analyze Token Usage**: Review `tokens` field:
   - **Input Tokens**: Size of requests (messages + tools)
   - **Output Tokens**: Size of responses
   - **Total Tokens**: Combined usage
   - **Efficiency**: Low output/input ratio may indicate inefficiency

**3. Compare Models**: Use `get_recent_stats` to:
   - See total calls per model
   - Combine with `search_traces` filtered by model to get costs
   - Identify which models are most expensive
   - Compare cost per call across models

**4. Detailed Cost Breakdown**: Use `get_llm_call` to:
   - See exact `costs` and `tokens` for specific calls
   - Review request size (messages, tools) affecting costs
   - Identify expensive patterns (long messages, many tools)

**5. Analyze Run Costs**: Use `get_run_overview` to:
   - See all LLM calls in a run (`llm_summaries`)
   - Use `get_llm_call` for each span to sum costs
   - Identify expensive runs
   - Find cost optimization opportunities

**Cost Optimization Strategies**:

**Reduce Input Tokens**:
- Shorten message history
- Simplify tool definitions
- Remove unnecessary context
- Use more efficient prompts

**Reduce Output Tokens**:
- Set `max_tokens` limits appropriately
- Use faster/cheaper models for simple tasks
- Cache frequent responses

**Model Selection**:
- Use cheaper models for simple tasks
- Reserve expensive models for complex tasks
- Compare cost vs quality trade-offs

**Tool Usage**:
- Minimize tool definitions when possible
- Cache tool results
- Optimize tool call frequency

**Common Cost Issues**:
- **Token Waste**: High input tokens with low value
- **Model Overuse**: Using expensive models unnecessarily
- **Tool Bloat**: Excessive tool definitions
- **Inefficient Prompts**: Long prompts that don't add value
- **No Caching**: Repeated identical calls

**Cost Analysis Checklist**:
1. Get overall costs using `search_traces` with cost include
2. Identify most expensive models
3. Find expensive individual calls
4. Analyze token efficiency
5. Compare costs across time periods
6. Identify optimization opportunities
"#
                    .to_string(),
                ),
            },
        ])
    }

    fn _create_resource_template(&self, uri: &str, name: &str) -> Annotated<RawResourceTemplate> {
        Annotated::new(RawResourceTemplate {
            uri_template: uri.to_string(),
            name: name.to_string(),
            title: Some(name.to_string()),
            description: None,
            mime_type: None
        }, None)
    }
}

#[tool_handler]
#[prompt_handler]
impl<T: TraceService + Send + Sync + 'static> ServerHandler for VlloraMcp<T> {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("This server provides debugging tools and prompts for analyzing LLM traces, runs, and system health based on Vllora traces.".to_string()),
        }
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                self._create_resource_template("run://{id}", "run"),
            ],
            meta: None,
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        ReadResourceRequestParam { uri }: ReadResourceRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let (resource_type, resource_id) = uri.split_once("://").unwrap();
        match resource_type {
            "run" => {
                let run = self.get_run_overview(Parameters(GetRunOverviewParams { run_id: resource_id.to_string() })).await.map_err(|e| McpError::internal_error(e, None))?;
                let run_json = serde_json::to_string(&run.0).unwrap();
                Ok(ReadResourceResult {
                    contents: vec![ResourceContents::text(run_json, uri)],
                })
            }
            _ => Err(McpError::resource_not_found(
                "resource_not_found",
                Some(json!({
                    "uri": uri
                })),
            )),
        }
    }
}
