use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
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

    /// Port to listen on (e.g., 8084)
    #[arg(long, value_name = "UI_PORT")]
    pub ui_port: Option<u16>,

    /// Comma-separated list of allowed CORS origins (e.g., http://localhost:3000,https://example.com)
    #[arg(long, value_name = "ORIGINS")]
    pub cors_origins: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the API server (default if no command specified)
    Serve(ServeArgs),
    /// List all available models
    List,
    /// Sync models and/or providers from API to database
    Sync {
        /// Sync only models
        #[arg(long)]
        models: bool,
        /// Sync only providers
        #[arg(long)]
        providers: bool,
    },
    /// Generate models JSON file for embedding
    #[command(hide = true)]
    GenerateModelsJson {
        /// Output file path (default: models_data.json)
        #[arg(short, long, default_value = "models_data.json")]
        output: String,
    },
}
