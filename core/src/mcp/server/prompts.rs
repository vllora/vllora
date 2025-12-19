/// Prompts module containing all MCP prompt templates.
///
/// Prompts are loaded from separate text files at compile time using `include_str!`.
#[derive(Clone)]
pub struct Prompts {
    pub debug_errors: &'static str,
    pub analyze_performance: &'static str,
    pub understand_run_flow: &'static str,
    pub search_traces_guide: &'static str,
    pub monitor_system_health: &'static str,
    pub analyze_costs: &'static str,
}

impl Prompts {
    /// Create a new Prompts instance with all prompts loaded from files.
    pub fn new() -> Self {
        Self {
            debug_errors: include_str!("prompts/debug_errors.txt"),
            analyze_performance: include_str!("prompts/analyze_performance.txt"),
            understand_run_flow: include_str!("prompts/understand_run_flow.txt"),
            search_traces_guide: include_str!("prompts/search_traces_guide.txt"),
            monitor_system_health: include_str!("prompts/monitor_system_health.txt"),
            analyze_costs: include_str!("prompts/analyze_costs.txt"),
        }
    }
}

impl Default for Prompts {
    fn default() -> Self {
        Self::new()
    }
}
