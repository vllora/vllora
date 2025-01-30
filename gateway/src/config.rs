use langdb_core::handler::middleware::rate_limit::RateLimiting;
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
    pub redis: Option<RedisConfig>,
    pub cost_control: Option<CostControl>,
    pub rate_limit: Option<RateLimiting>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CostControl {
    pub daily: Option<f64>,
    pub monthly: Option<f64>,
    pub total: Option<f64>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(crate = "serde", deny_unknown_fields)]
pub struct RedisConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
}
impl Default for RedisConfig {
    fn default() -> Self {
        RedisConfig {
            url: default_redis_url(),
        }
    }
}

fn default_redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or("redis://localhost:6379".to_string())
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
