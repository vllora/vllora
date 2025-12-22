use crate::CliError;
use chrono::TimeZone;
use prettytable::{row, Table};
use vllora_core::mcp::server::tools::{
    SearchTracesFilters, SearchTracesOperationKind, SearchTracesPage, SearchTracesParams,
    SearchTracesResponse, SearchTracesSort, SearchTracesSortOrder, SearchTracesStatus,
    SearchTracesTimeRange,
};
use vllora_core::mcp::server::VlloraMcp;
use vllora_core::metadata::services::trace::TraceServiceImpl as MetadataTraceServiceImpl;
use vllora_core::rmcp;

type VlloraMcpInstance = VlloraMcp<MetadataTraceServiceImpl>;

pub fn format_traces_table(response: &SearchTracesResponse) {
    let mut table = Table::new();

    // Add header row
    table.add_row(row![bF=>
        "Trace ID",
        "Span ID",
        "Operation",
        "Status",
        "Duration (ms)",
        "Start Time",
        "Run ID",
        "Thread ID",
    ]);

    // Add data rows
    for item in &response.items {
        // Format start time (convert from microseconds timestamp string to readable format)
        let start_time_str = item.start_time.clone();
        let start_time_display = if let Ok(timestamp_us) = start_time_str.parse::<i64>() {
            let secs = timestamp_us / 1_000_000;
            let micros = (timestamp_us % 1_000_000) as u32;
            if let Some(dt) = chrono::Utc.timestamp_opt(secs, micros * 1_000).single() {
                dt.format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                start_time_str
            }
        } else {
            start_time_str
        };

        // Format status with emoji
        let status_display = match item.status {
            SearchTracesStatus::Ok => "✓ OK".to_string(),
            SearchTracesStatus::Error => "✗ Error".to_string(),
            SearchTracesStatus::Any => "?".to_string(),
        };

        let run_id = item
            .run_id
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "-".to_string());
        let thread_id = item
            .thread_id
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "-".to_string());

        table.add_row(row![
            item.trace_id,
            item.span_id,
            item.root_operation_name,
            status_display,
            item.duration_ms,
            start_time_display,
            run_id,
            thread_id,
        ]);
    }

    // Print the table
    table.printstd();

    // Print pagination info if available
    if let Some(next_cursor) = &response.next_cursor {
        println!("\nNext cursor: {}", next_cursor);
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_list(
    vllora_mcp: &VlloraMcpInstance,
    limit: i64,
    offset: i64,
    run_id: Option<String>,
    thread_id: Option<String>,
    operation_name: Option<String>,
    text: Option<String>,
    last_n_minutes: Option<i64>,
    sort_by: String,
    sort_order: String,
    output: String,
) -> Result<(), CliError> {
    // Build SearchTracesParams from CLI arguments
    let filters = SearchTracesFilters {
        project_id: None,
        thread_id,
        run_id,
        status: None,
        model: None,
        operation_name: operation_name.map(|op| {
            // Map string to SearchTracesOperationKind
            match op.as_str() {
                "run" => SearchTracesOperationKind::Run,
                "agent" => SearchTracesOperationKind::Agent,
                "task" => SearchTracesOperationKind::Task,
                "tools" => SearchTracesOperationKind::Tools,
                "openai" => SearchTracesOperationKind::Openai,
                "anthropic" => SearchTracesOperationKind::Anthropic,
                "bedrock" => SearchTracesOperationKind::Bedrock,
                "gemini" => SearchTracesOperationKind::Gemini,
                "cloud_api_invoke" => SearchTracesOperationKind::CloudApiInvoke,
                "api_invoke" => SearchTracesOperationKind::ApiInvoke,
                "model_call" | "llm_call" => SearchTracesOperationKind::ModelCall,
                "tool_call" => SearchTracesOperationKind::ToolCall,
                _ => SearchTracesOperationKind::ModelCall, // default
            }
        }),
        labels: None,
        text,
        has_thread: None,
        has_run: None,
    };

    let filters = if filters.run_id.is_none()
        && filters.thread_id.is_none()
        && filters.operation_name.is_none()
        && filters.text.is_none()
    {
        None
    } else {
        Some(filters)
    };

    let time_range = last_n_minutes.map(|minutes| SearchTracesTimeRange {
        last_n_minutes: Some(minutes),
        since: None,
        until: None,
    });

    let sort = Some(SearchTracesSort {
        by: sort_by,
        order: Some(match sort_order.as_str() {
            "asc" => SearchTracesSortOrder::Asc,
            _ => SearchTracesSortOrder::Desc,
        }),
    });

    let page = Some(SearchTracesPage {
        limit,
        offset: Some(offset),
    });

    let params = SearchTracesParams {
        time_range,
        filters,
        sort,
        page,
        include: None,
    };

    // Call VlloraMcp::search_traces
    let result = vllora_mcp
        .search_traces(rmcp::handler::server::wrapper::Parameters(params))
        .await
        .map_err(|e| CliError::CustomError(e.to_string()))?;

    // Format output based on user preference
    match output.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result.0)?);
        }
        _ => {
            format_traces_table(&result.0);
        }
    }
    Ok(())
}
