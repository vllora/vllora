use crate::CliError;
use prettytable::{row, Table};
use vllora_core::mcp::server::tools::{GetRunOverviewParams, GetRunOverviewResponse};
use vllora_core::mcp::server::VlloraMcp;
use vllora_core::metadata::services::trace::TraceServiceImpl as MetadataTraceServiceImpl;
use vllora_core::rmcp;

type VlloraMcpInstance = VlloraMcp<MetadataTraceServiceImpl>;

pub fn format_run_overview_table(response: &GetRunOverviewResponse) {
    // Run metadata table
    let mut run_table = Table::new();
    run_table.add_row(row![bF=> "Field", "Value"]);

    let run = &response.run;
    run_table.add_row(row!["Run ID", run.run_id]);
    run_table.add_row(row!["Status", run.status]);
    run_table.add_row(row!["Start Time", run.start_time]);
    run_table.add_row(row!["Duration", format!("{} ms", run.duration_ms)]);
    run_table.add_row(row!["Root Span ID", run.root_span_id]);
    run_table.add_row(row!["Total LLM Calls", run.total_llm_calls]);

    // Add total cost if available
    if let Some(cost) = run.total_cost {
        run_table.add_row(row!["Total Cost", format!("${:.6}", cost)]);
    }

    // Add usage information if available
    if let Some(ref usage) = run.usage {
        let usage_summary = format!(
            "Input: {}, Output: {}, Total: {}",
            usage.input_tokens, usage.output_tokens, usage.total_tokens
        );
        run_table.add_row(row!["Token Usage", usage_summary]);

        if usage.is_cache_used {
            run_table.add_row(row!["Cache Used", "Yes"]);
        }

        // Add prompt tokens details if available
        if let Some(ref prompt_details) = usage.prompt_tokens_details {
            let prompt_info = format!(
                "Cached: {}, Cache Creation: {}, Audio: {}",
                prompt_details.cached_tokens(),
                prompt_details.cache_creation_tokens(),
                prompt_details.audio_tokens()
            );
            run_table.add_row(row!["Prompt Details", prompt_info]);
        }

        // Add completion tokens details if available
        if let Some(ref completion_details) = usage.completion_tokens_details {
            let completion_info = format!(
                "Accepted: {}, Audio: {}, Reasoning: {}, Rejected: {}",
                completion_details.accepted_prediction_tokens(),
                completion_details.audio_tokens(),
                completion_details.reasoning_tokens(),
                completion_details.rejected_prediction_tokens()
            );
            run_table.add_row(row!["Completion Details", completion_info]);
        }
    }

    if let Some(label) = &run.label {
        if !label.is_empty() {
            let label_str = label
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join(", ");
            run_table.add_row(row!["Labels", label_str]);
        }
    }

    println!("Run Overview:");
    run_table.printstd();

    // Spans table
    if !response.span_tree.is_empty() {
        let mut spans_table = Table::new();
        spans_table.add_row(row![bF=> "Span ID", "Parent", "Operation", "Kind", "Status"]);

        for span in &response.span_tree {
            let parent = span.parent_span_id.as_deref().unwrap_or("-");
            spans_table.add_row(row![
                span.span_id,
                parent,
                span.operation_name,
                span.kind,
                span.status,
            ]);
        }

        println!("\nSpans ({}):", response.span_tree.len());
        spans_table.printstd();
    }

    // Agents used
    if !response.agents_used.is_empty() {
        println!("\nAgents Used ({}):", response.agents_used.len());
        for agent in &response.agents_used {
            println!("  - {}", agent);
        }
    }

    // Error breadcrumbs
    if !response.error_breadcrumbs.is_empty() {
        let mut errors_table = Table::new();
        errors_table.add_row(row![bF=> "Span ID", "Operation", "Error"]);

        for error in &response.error_breadcrumbs {
            let error_msg = error.error.as_deref().unwrap_or("-");
            errors_table.add_row(row![error.span_id, error.operation_name, error_msg,]);
        }

        println!("\nErrors ({}):", response.error_breadcrumbs.len());
        errors_table.printstd();
    }

    // LLM summaries
    if !response.llm_summaries.is_empty() {
        let mut llm_table = Table::new();
        llm_table.add_row(row![bF=> "Span ID", "Provider", "Model", "Messages", "Tools"]);

        for llm in &response.llm_summaries {
            let provider = llm.provider.as_deref().unwrap_or("-");
            let model = llm.model.as_deref().unwrap_or("-");
            llm_table.add_row(row![
                llm.span_id,
                provider,
                model,
                llm.message_count,
                llm.tool_count,
            ]);
        }

        println!("\nLLM Calls ({}):", response.llm_summaries.len());
        llm_table.printstd();
    }

    // Tool summaries
    if !response.tool_summaries.is_empty() {
        let mut tools_table = Table::new();
        tools_table.add_row(row![bF=> "Span ID", "Tool Name", "Status"]);

        for tool in &response.tool_summaries {
            let tool_name = tool.tool_name.as_deref().unwrap_or("-");
            tools_table.add_row(row![tool.span_id, tool_name, tool.status,]);
        }

        println!("\nTool Calls ({}):", response.tool_summaries.len());
        tools_table.printstd();
    }
}

pub async fn handle_run_info(
    vllora_mcp: &VlloraMcpInstance,
    run_id: String,
    output: String,
) -> Result<(), CliError> {
    let params = GetRunOverviewParams { run_id };

    // Call VlloraMcp::get_run_overview
    let result = vllora_mcp
        .get_run_overview(rmcp::handler::server::wrapper::Parameters(params))
        .await
        .map_err(|e| CliError::CustomError(e.to_string()))?;

    // Format output based on user preference
    match output.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result.0)?);
        }
        _ => {
            format_run_overview_table(&result.0);
        }
    }
    Ok(())
}
