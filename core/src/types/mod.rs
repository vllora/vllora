pub mod aws;
pub mod credentials;
pub mod db_connection;
pub mod embed;
pub mod engine;
pub mod gateway;
pub mod image;
pub mod json;
pub mod message;
pub mod provider;
pub mod threads;

pub const LANGDB_API_URL: &str = "https://api.us-east-1.langdb.ai/v1";
pub const LANGDB_UI_URL: &str = "https://app.langdb.ai";

#[derive(Clone, Debug)]
pub struct GatewayTenant {
    pub name: String,
    pub project_slug: String,
}
