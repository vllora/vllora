use crate::cli;
use crate::session::Credentials;
use langdb_core::executor::ProvidersConfig;
use langdb_core::handler::middleware::rate_limit::RateLimiting;
use langdb_core::types::credentials::ApiKeyCredentials;
use langdb_core::types::guardrails::Guard;
use minijinja::Environment;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to parse config file. Error: {0}")]
    ParseError(#[from] serde_yaml::Error),
    #[error("Failed to read template in config. Error: {0}")]
    ReadError(#[from] minijinja::Error),
}

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
    #[serde(default)]
    pub http: HttpConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub clickhouse: Option<ClickhouseConfig>,
    #[serde(default)]
    pub cost_control: Option<CostControl>,
    #[serde(default)]
    pub rate_limit: Option<RateLimiting>,
    #[serde(default)]
    pub providers: Option<ProvidersConfig>,
    #[serde(default)]
    pub guards: Option<HashMap<String, Guard>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UiConfig {
    pub port: u16,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self { port: 8084 }
    }
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
            host: "0.0.0.0".to_string(),
            port: 9090,
            cors_allowed_origins: vec!["*".to_string()],
        }
    }
}

fn replace_env_vars(content: String) -> Result<String, ConfigError> {
    let env = Environment::new();
    let template = env.template_from_str(&content)?;
    let parameters = template.undeclared_variables(false);

    let mut variables = HashMap::new();
    parameters.iter().for_each(|k| {
        if let Ok(v) = std::env::var(k) {
            variables.insert(k, v);
        };
    });

    Ok(template.render(variables)?)
}

impl Config {
    pub fn load<P: AsRef<Path>>(config_path: P) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(config_path) {
            Ok(content) => {
                let content = replace_env_vars(content)?;
                Ok(serde_yaml::from_str(&content)?)
            }
            Err(_e) => Ok(Self::default()),
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

            // Apply UI config overrides
            if let Some(port) = args.ui_port {
                self.ui.port = port;
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

pub fn load_langdb_proxy_config(config: Option<ProvidersConfig>) -> Option<ProvidersConfig> {
    let langdb_api_key = std::env::var("LANGDB_KEY").ok().or_else(|| {
        std::env::var("HOME")
            .ok()
            .and_then(|home_dir| {
                let credentials_path = format!("{home_dir}/.langdb/credentials.yaml");
                std::fs::read_to_string(credentials_path).ok()
            })
            .and_then(|credentials| serde_yaml::from_str::<Credentials>(&credentials).ok())
            .map(|credentials| credentials.api_key)
    });

    if let Some(key) = langdb_api_key {
        if let Some(mut providers_config) = config {
            if !providers_config.0.contains_key("langdb_proxy") {
                providers_config.0.insert(
                    "langdb_proxy".to_string(),
                    ApiKeyCredentials { api_key: key },
                );
            }
            Some(providers_config)
        } else {
            Some(ProvidersConfig(HashMap::from([(
                "langdb_proxy".to_string(),
                ApiKeyCredentials { api_key: key },
            )])))
        }
    } else {
        config
    }
}
