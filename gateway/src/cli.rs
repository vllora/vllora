use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    /// Optional config file path
    #[arg(short, long, default_value = "config.yaml")]
    pub config: String,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Clone, Parser, Default)]
pub struct ServeArgs {
    /// Host address to bind to (e.g., 127.0.0.1 for local or 0.0.0.0 for all interfaces)
    #[arg(long, value_name = "ADDRESS")]
    pub host: Option<String>,

    /// Port to listen on (e.g., 8080)
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// Comma-separated list of allowed CORS origins (e.g., http://localhost:3000,https://example.com)
    #[arg(long, value_name = "ORIGINS")]
    pub cors_origins: Option<String>,

    /// Clickhouse database URL (e.g., clickhouse://localhost:9000)
    #[arg(long, value_name = "URL")]
    pub clickhouse_url: Option<String>,

    /// Daily cost limit in dollars (e.g., 100.0)
    #[arg(long, value_name = "LIMIT")]
    pub cost_daily: Option<f64>,

    /// Monthly cost limit in dollars (e.g., 1000.0)
    #[arg(long, value_name = "LIMIT")]
    pub cost_monthly: Option<f64>,

    /// Total cost limit in dollars (e.g., 5000.0)
    #[arg(long, value_name = "LIMIT")]
    pub cost_total: Option<f64>,

    /// Maximum number of API calls per hour (e.g., 1000)
    #[arg(long, value_name = "LIMIT")]
    pub rate_hourly: Option<u64>,

    /// Maximum number of API calls per day (e.g., 10000)
    #[arg(long, value_name = "LIMIT")]
    pub rate_daily: Option<u64>,

    /// Maximum number of API calls per month (e.g., 100000)
    #[arg(long, value_name = "LIMIT")]
    pub rate_monthly: Option<u64>,

    /// Start server in interactive mode with TUI interface
    #[arg(short, long)]
    pub interactive: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Update the available models cache
    Update {
        /// Force update even if cache is fresh
        #[arg(short, long)]
        force: bool,
    },
    /// List all available models
    List,
    /// Start the API server (default if no command specified)
    Serve(ServeArgs),
}
