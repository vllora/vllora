use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RestConfig {
    pub host: String,
    pub port: u16,
    pub cors_allowed_origins: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone, Default)]
#[serde(crate = "serde")]
pub struct ClickhouseConfig {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub rest: RestConfig,
    pub clickhouse: Option<ClickhouseConfig>,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            cors_allowed_origins: vec!["*".to_string()],
        }
    }
}

impl Config {
    pub fn load<P: AsRef<Path>>(config_path: P) -> Self {
        match std::fs::File::open(config_path) {
            Ok(f) => serde_yaml::from_reader(f).unwrap(),
            Err(_) => Self::default(),
        }
    }
}
