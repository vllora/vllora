use super::shared::{
    generate_messages, generate_response_messages, map_request, map_response, Message,
};
use crate::CliError;
use chrono::TimeZone;
use prettytable::{row, Table};
use vllora_core::mcp::server::tools::{GetLlmCallInclude, GetLlmCallParams, GetLlmCallResponse};
use vllora_core::mcp::server::VlloraMcp;
use vllora_core::metadata::services::trace::TraceServiceImpl as MetadataTraceServiceImpl;
use vllora_core::rmcp;

type VlloraMcpInstance = VlloraMcp<MetadataTraceServiceImpl>;

pub fn format_llm_call_table(
    response: &GetLlmCallResponse,
    trace_id: Option<&String>,
    run_id: Option<&String>,
    thread_id: Option<&String>,
    start_time: Option<&String>,
    duration_ms: Option<i64>,
) {
    // Metadata table
    let mut metadata_table = Table::new();
    metadata_table.add_row(row![bF=> "Field", "Value"]);

    // Trace ID
    if let Some(trace_id) = trace_id {
        metadata_table.add_row(row!["Trace ID", trace_id,]);
    }

    // Run ID
    if let Some(run_id) = run_id {
        metadata_table.add_row(row!["Run ID", run_id,]);
    }

    // Thread ID
    if let Some(thread_id) = thread_id {
        metadata_table.add_row(row!["Thread ID", thread_id,]);
    }

    // Span ID
    metadata_table.add_row(row!["Span ID", response.span_id,]);

    // Start Time
    if let Some(start_time) = start_time {
        // Format start time if it's a timestamp string
        let start_time_display = if let Ok(timestamp_us) = start_time.parse::<i64>() {
            let secs = timestamp_us / 1_000_000;
            let micros = (timestamp_us % 1_000_000) as u32;
            if let Some(dt) = chrono::Utc.timestamp_opt(secs, micros * 1_000).single() {
                dt.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string()
            } else {
                start_time.clone()
            }
        } else {
            start_time.clone()
        };
        metadata_table.add_row(row!["Start Time", start_time_display,]);
    }

    // Duration
    if let Some(duration_ms) = duration_ms {
        metadata_table.add_row(row!["Duration", format!("{} ms", duration_ms),]);
    }

    // Provider
    if let Some(provider) = &response.provider {
        metadata_table.add_row(row!["Provider", provider,]);
    }

    // Model (from request)
    if let Some(request) = &response.request {
        if let Some(model) = &request.model {
            metadata_table.add_row(row!["Model", model,]);
        }

        // Model parameters
        if let Some(params) = &request.params {
            metadata_table.add_row(row![
                "Parameters",
                serde_json::to_string(params).unwrap_or_else(|_| "N/A".to_string()),
            ]);
        }

        // Messages count
        if let Some(messages) = &request.messages {
            let msg_count = if let Some(arr) = messages.as_array() {
                arr.len().to_string()
            } else {
                "N/A".to_string()
            };
            metadata_table.add_row(row!["Messages Count", msg_count,]);
        }

        // Tools count
        if let Some(tools) = &request.tools {
            let tool_count = if let Some(arr) = tools.as_array() {
                arr.len().to_string()
            } else {
                "N/A".to_string()
            };
            metadata_table.add_row(row!["Tools Count", tool_count,]);
        }
    }

    // Tokens
    if let Some(tokens) = &response.tokens {
        let t = if let Some(obj) = tokens.as_str() {
            serde_json::from_str(obj).unwrap_or(tokens.clone())
        } else {
            tokens.clone()
        };

        let token_str = if let Some(obj) = t.as_object() {
            let mut parts = Vec::new();
            if let Some(input) = obj.get("input_tokens").or_else(|| obj.get("prompt_tokens")) {
                parts.push(format!("Input: {}", input));
            }
            if let Some(output) = obj
                .get("output_tokens")
                .or_else(|| obj.get("completion_tokens"))
            {
                parts.push(format!("Output: {}", output));
            }
            if let Some(total) = obj.get("total_tokens") {
                parts.push(format!("Total: {}", total));
            }
            if parts.is_empty() {
                serde_json::to_string(&tokens).unwrap_or_else(|_| "N/A".to_string())
            } else {
                parts.join(", ")
            }
        } else {
            serde_json::to_string(&t).unwrap_or_else(|_| "N/A".to_string())
        };
        metadata_table.add_row(row!["Tokens", token_str,]);
    }

    // Costs
    if let Some(costs) = &response.costs {
        let cost_str = if let Some(num) = costs.as_f64() {
            format!("{:.6}", num)
        } else if let Some(num) = costs.as_i64() {
            format!("{}", num)
        } else if let Some(obj) = costs.as_object() {
            let mut parts = Vec::new();

            // Helper function to parse and format cost values
            let format_cost_value = |value: &serde_json::Value| -> String {
                if let Some(num) = value.as_f64() {
                    format!("{:.6}", num)
                } else if let Some(num) = value.as_i64() {
                    format!("{}", num)
                } else if let Some(str_val) = value.as_str() {
                    // Try to parse string as float
                    str_val
                        .parse::<f64>()
                        .map(|f| format!("{:.6}", f))
                        .unwrap_or_else(|_| str_val.to_string())
                } else {
                    value.to_string().trim_matches('"').to_string()
                }
            };

            if let Some(input) = obj.get("input_cost") {
                parts.push(format!("Input: {}", format_cost_value(input)));
            }
            if let Some(output) = obj.get("output_cost") {
                parts.push(format!("Output: {}", format_cost_value(output)));
            }
            if let Some(total) = obj.get("total_cost") {
                parts.push(format!("Total: {}", format_cost_value(total)));
            }
            if parts.is_empty() {
                serde_json::to_string(costs)
                    .unwrap_or_else(|_| "N/A".to_string())
                    .trim_matches('"')
                    .to_string()
            } else {
                parts.join(", ")
            }
        } else if let Some(str_val) = costs.as_str() {
            // Try to parse string as float
            str_val
                .parse::<f64>()
                .map(|f| format!("{:.6}", f))
                .unwrap_or_else(|_| str_val.trim_matches('"').to_string())
        } else {
            costs.to_string().trim_matches('"').to_string()
        };
        metadata_table.add_row(row!["Cost", cost_str,]);
    }

    // Redactions
    if let Some(redactions) = &response.redactions {
        if !redactions.is_empty() {
            metadata_table.add_row(row![
                "Redactions",
                format!("{} redaction(s) applied", redactions.len()),
            ]);
        }
    }

    // Print metadata table
    println!("Metadata:");
    metadata_table.printstd();

    // Conversation table
    let mut conversation_table = Table::new();
    conversation_table.add_row(row![bF=> "Role/Type", "Content"]);

    let mut messages = Vec::new();
    // Raw Request (if available)
    if let Some(raw_request) = &response.raw_request {
        let request = map_request(raw_request);
        let generated_messages = generate_messages(&request);
        messages.extend(generated_messages);
    }

    // Raw Response (if available)
    if let Some(raw_response) = &response.raw_response {
        let response = map_response(raw_response);
        let generated_messages = generate_response_messages(&response);
        messages.extend(generated_messages);
    }

    // Add messages to conversation table
    for message in &messages {
        match message {
            Message::Text(role, content) => {
                conversation_table.add_row(row![role, content]);
            }
            Message::ToolCall {
                name,
                id,
                arguments,
            } => {
                let tool_call_name = if let Some(id) = id {
                    format!("tool_call: {} ({})", name, id)
                } else {
                    format!("tool_call: {}", name)
                };
                let args_str = serde_json::to_string_pretty(arguments)
                    .unwrap_or_else(|_| format!("{:?}", arguments));
                conversation_table.add_row(row![tool_call_name, args_str]);
            }
        }
    }

    // Print conversation table if there are messages
    if !messages.is_empty() {
        println!("\nConversation:");
        conversation_table.printstd();
    }

    // Print additional info if request/response data is available
    if response.request.is_some() || response.response.is_some() {
        println!("\n💡 Tip: Use --output json to see full request/response payloads");
    }
}

pub async fn handle_call_info(
    vllora_mcp: &VlloraMcpInstance,
    span_id: String,
    output: String,
) -> Result<(), CliError> {
    let include: Option<GetLlmCallInclude> = Some(GetLlmCallInclude {
        llm_payload: false,
        unsafe_text: false,
        raw_request: true,
        raw_response: true,
    });

    let params = GetLlmCallParams {
        span_id: span_id.clone(),
        allow_unsafe_text: false,
        include,
    };

    // Call VlloraMcp::get_llm_call
    let result = vllora_mcp
        .get_llm_call(rmcp::handler::server::wrapper::Parameters(params))
        .await
        .map_err(|e| CliError::CustomError(e.to_string()))?;

    // Format output based on user preference
    match output.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result.0)?);
        }
        _ => {
            let trace_id_ref = result.0.trace_id.as_ref();
            let run_id_ref = result.0.run_id.as_ref();
            let thread_id_ref = result.0.thread_id.as_ref();
            let start_time_ref = result.0.start_time.as_ref();
            let duration_ms = result.0.duration_ms;

            format_llm_call_table(
                &result.0,
                trace_id_ref,
                run_id_ref,
                thread_id_ref,
                start_time_ref,
                duration_ms,
            );
        }
    }
    Ok(())
}
