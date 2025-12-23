use crate::CliError;
use clap::Subcommand;
use vllora_core::mcp::server::VlloraMcp;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::trace::TraceServiceImpl as MetadataTraceServiceImpl;
use vllora_core::metadata::DatabaseServiceTrait;

mod call_info;
mod list;
mod overview;
mod run_info;

#[derive(Subcommand)]
pub enum TracesCommands {
    /// Search/list traces
    List {
        /// Limit number of results
        #[arg(long, default_value_t = 20)]
        limit: i64,
        /// Offset for pagination
        #[arg(long, default_value_t = 0)]
        offset: i64,
        /// Filter by run ID
        #[arg(long)]
        run_id: Option<String>,
        /// Filter by thread ID
        #[arg(long)]
        thread_id: Option<String>,
        /// Filter by operation name (run, agent, task, tools, openai, anthropic, bedrock, gemini, model_call)
        #[arg(long)]
        operation_name: Option<String>,
        /// Text search query
        #[arg(long)]
        text: Option<String>,
        /// Filter traces from last N minutes
        #[arg(long)]
        last_n_minutes: Option<i64>,
        /// Sort by field (default: start_time)
        #[arg(long, default_value = "start_time")]
        sort_by: String,
        /// Sort order (asc or desc)
        #[arg(long, default_value = "desc")]
        sort_order: String,
        /// Output format (table or json)
        #[arg(long, default_value = "table")]
        output: String,
    },
    /// Get detailed LLM call information for a span
    CallInfo {
        /// Span ID
        #[arg(long)]
        span_id: String,
        /// Output format (table or json)
        #[arg(long, default_value = "table")]
        output: String,
    },
    /// Get overview of a run and its spans
    RunInfo {
        /// Run ID
        #[arg(long)]
        run_id: String,
        /// Output format (table or json)
        #[arg(long, default_value = "table")]
        output: String,
    },
    /// Get aggregated stats for recent LLM and tool calls
    Overview {
        /// Number of minutes in the past to include
        #[arg(long)]
        last_n_minutes: i64,
        /// Output format (table or json)
        #[arg(long, default_value = "table")]
        output: String,
    },
}

pub async fn handle_traces(db_pool: DbPool, cmd: TracesCommands) -> Result<(), CliError> {
    // Create VlloraMcp instance with the trace service
    let trace_service = MetadataTraceServiceImpl::init(db_pool.clone());
    let vllora_mcp = VlloraMcp::new(trace_service);

    match cmd {
        TracesCommands::List {
            limit,
            offset,
            run_id,
            thread_id,
            operation_name,
            text,
            last_n_minutes,
            sort_by,
            sort_order,
            output,
        } => {
            list::handle_list(
                &vllora_mcp,
                limit,
                offset,
                run_id,
                thread_id,
                operation_name,
                text,
                last_n_minutes,
                sort_by,
                sort_order,
                output,
            )
            .await
        }
        TracesCommands::CallInfo { span_id, output } => {
            call_info::handle_call_info(&vllora_mcp, span_id, output).await
        }
        TracesCommands::RunInfo { run_id, output } => {
            run_info::handle_run_info(&vllora_mcp, run_id, output).await
        }
        TracesCommands::Overview {
            last_n_minutes,
            output,
        } => overview::handle_overview(&vllora_mcp, last_n_minutes, output).await,
    }
}
