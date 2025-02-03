use crate::cli;
use langdb_core::executor::ProvidersConfig;
use langdb_core::handler::middleware::rate_limit::RateLimiting;
use minijinja::Environment;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HttpConfig {
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
    pub http: HttpConfig,
    pub clickhouse: Option<ClickhouseConfig>,
    pub cost_control: Option<CostControl>,
    pub rate_limit: Option<RateLimiting>,
    pub providers: Option<ProvidersConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CostControl {
    pub daily: Option<f64>,
    pub monthly: Option<f64>,
    pub total: Option<f64>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            cors_allowed_origins: vec!["*".to_string()],
        }
    }
}

fn replace_env_vars(content: String) -> String {
    let env = Environment::new();
    let template = env.template_from_str(&content).unwrap();
    let parameters = template.undeclared_variables(false);

    let mut variables = HashMap::new();
    parameters.iter().for_each(|k| {
        if let Ok(v) = std::env::var(k) {
            variables.insert(k, v);
        };
    });

    template.render(variables).unwrap()
}

impl Config {
    pub fn load<P: AsRef<Path>>(config_path: P) -> Self {
        match std::fs::read_to_string(config_path) {
            Ok(content) => {
                let content = replace_env_vars(content);
                serde_yaml::from_str(&content).unwrap()
            }
            Err(_) => Self::default(),
        }
    }

    pub fn apply_cli_overrides(mut self, cli_opts: &cli::Commands) -> Self {
        if let cli::Commands::Serve(args) = cli_opts {
            // Apply REST config overrides
            if let Some(host) = &args.host {
                self.http.host = host.clone();
            }
            if let Some(port) = args.port {
                self.http.port = port;
            }
            if let Some(cors) = &args.cors_origins {
                self.http.cors_allowed_origins =
                    cors.split(',').map(|s| s.trim().to_string()).collect();
            }

            // Apply Clickhouse config override
            if let Some(url) = &args.clickhouse_url {
                self.clickhouse = Some(ClickhouseConfig { url: url.clone() });
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
