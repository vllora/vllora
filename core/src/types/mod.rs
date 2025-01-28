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

#[derive(Clone, Debug)]
pub struct GatewayTenant {
    pub name: String,
    pub project_slug: String,
}
