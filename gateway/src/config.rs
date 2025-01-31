use crate::cli;
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

    pub fn apply_cli_overrides(mut self, cli_opts: &cli::Commands) -> Self {
        if let cli::Commands::Serve(args) = cli_opts {
            // Apply REST config overrides
            if let Some(host) = &args.host {
                self.rest.host = host.clone();
            }
            if let Some(port) = args.port {
                self.rest.port = port;
            }
            if let Some(cors) = &args.cors_origins {
                self.rest.cors_allowed_origins =
                    cors.split(',').map(|s| s.trim().to_string()).collect();
            }

            // Apply Clickhouse config override
            if let Some(url) = &args.clickhouse_url {
                self.clickhouse = Some(ClickhouseConfig { url: url.clone() });
            }

            // Apply Redis config override
            if let Some(url) = &args.redis_url {
                self.redis = Some(RedisConfig { url: url.clone() });
            }

            // Apply cost control overrides
            let mut cost_control = self.cost_control.unwrap_or_default();
            if let Some(daily) = args.cost_daily {
                cost_control.daily = Some(daily);
            }
            if let Some(monthly) = args.cost_monthly {
                cost_control.monthly = Some(monthly);
            }
            if let Some(total) = args.cost_total {
                cost_control.total = Some(total);
            }
            self.cost_control = Some(cost_control);

            // Apply rate limit overrides
            let mut rate_limit = self.rate_limit.unwrap_or_default();
            if let Some(hourly) = args.rate_hourly {
                rate_limit.hourly = Some(hourly);
            }
            if let Some(daily) = args.rate_daily {
                rate_limit.daily = Some(daily);
            }
            if let Some(monthly) = args.rate_monthly {
                rate_limit.monthly = Some(monthly);
            }
            self.rate_limit = Some(rate_limit);
        }
        self
    }
}
