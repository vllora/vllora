use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ConnectionDetails {
    Clickhouse(ClickhouseConnectionDetails),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClickhouseConnectionDetails {
    pub protocol: String,
    pub host: String,
    pub username: String,
    pub password: Option<String>,
    pub database: String,
    pub ssh: Option<SshSettings>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SshSettings {
    pub host: String,
    pub username: String,
    pub private_key: String,
    pub jump_servers: Vec<String>, // Optional jump server
}
