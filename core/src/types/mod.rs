pub mod embed;
pub mod guardrails;
pub mod handlers;
pub mod http;
pub mod image;
pub mod mcp;
pub mod metadata;
pub mod project_settings;
pub mod threads;
pub mod traces;

pub const LANGDB_API_URL: &str = "https://api.us-east-1.langdb.ai/v1";
pub const LANGDB_UI_URL: &str = "https://app.langdb.ai";

#[derive(Clone, Debug)]
pub struct GatewayTenant {
    pub name: String,
    pub project_slug: String,
}
