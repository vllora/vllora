use crate::CliError;
use prettytable::{row, Table};
use vllora_core::mcp::server::tools::{GetRecentOverviewParams, GetRecentOverviewResponse};
use vllora_core::mcp::server::VlloraMcp;
use vllora_core::metadata::services::trace::TraceServiceImpl as MetadataTraceServiceImpl;
use vllora_core::rmcp;

type VlloraMcpInstance = VlloraMcp<MetadataTraceServiceImpl>;

pub fn format_recent_stats_table(response: &GetRecentOverviewResponse) {
    // Time window info
    let mut window_table = Table::new();
    window_table.add_row(row![bF=> "Field", "Value"]);
    window_table.add_row(row![
        "Window Size",
        format!("{} minutes", response.window_minutes)
    ]);
    window_table.add_row(row!["Window Start", response.window_start]);
    window_table.add_row(row!["Window End", response.window_end]);

    println!("Time Window:");
    window_table.printstd();

    // LLM calls statistics
    if !response.llm_calls.is_empty() {
        let mut llm_table = Table::new();
        llm_table.add_row(row![bF=> "Model", "OK", "Errors", "Total"]);

        for llm in &response.llm_calls {
            llm_table.add_row(row![
                llm.model,
                llm.ok_count,
                llm.error_count,
                llm.total_count,
            ]);
        }

        // Calculate totals
        let total_ok: i64 = response.llm_calls.iter().map(|l| l.ok_count).sum();
        let total_errors: i64 = response.llm_calls.iter().map(|l| l.error_count).sum();
        let total_calls: i64 = response.llm_calls.iter().map(|l| l.total_count).sum();

        llm_table.add_row(row![
            bF-> "TOTAL",
            total_ok,
            total_errors,
            total_calls,
        ]);

        println!(
            "\nLLM Calls by Model ({} models):",
            response.llm_calls.len()
        );
        llm_table.printstd();
    } else {
        println!("\nLLM Calls: None");
    }

    // Tool calls statistics
    if !response.tool_calls.is_empty() {
        let mut tools_table = Table::new();
        tools_table.add_row(row![bF=> "Tool Name", "OK", "Errors", "Total"]);

        for tool in &response.tool_calls {
            tools_table.add_row(row![
                tool.tool_name,
                tool.ok_count,
                tool.error_count,
                tool.total_count,
            ]);
        }

        // Calculate totals
        let total_ok: i64 = response.tool_calls.iter().map(|t| t.ok_count).sum();
        let total_errors: i64 = response.tool_calls.iter().map(|t| t.error_count).sum();
        let total_calls: i64 = response.tool_calls.iter().map(|t| t.total_count).sum();

        tools_table.add_row(row![
            bF-> "TOTAL",
            total_ok,
            total_errors,
            total_calls,
        ]);

        println!(
            "\nTool Calls by Tool ({} tools):",
            response.tool_calls.len()
        );
        tools_table.printstd();
    } else {
        println!("\nTool Calls: None");
    }
}

pub async fn handle_overview(
    vllora_mcp: &VlloraMcpInstance,
    last_n_minutes: i64,
    output: String,
) -> Result<(), CliError> {
    let params = GetRecentOverviewParams { last_n_minutes };

    // Call VlloraMcp::get_recent_stats
    let result = vllora_mcp
        .get_recent_stats(rmcp::handler::server::wrapper::Parameters(params))
        .await
        .map_err(|e| CliError::CustomError(e.to_string()))?;

    // Format output based on user preference
    match output.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result.0)?);
        }
        _ => {
            format_recent_stats_table(&result.0);
        }
    }
    Ok(())
}
