use crate::cli;
use minijinja::Environment;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;
use vllora_core::executor::ProvidersConfig;
use vllora_core::types::guardrails::Guard;

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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    #[serde(default)]
    pub http: HttpConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub otel: OTelConfig,
    #[serde(default)]
    pub providers: Option<ProvidersConfig>,
    #[serde(default)]
    pub guards: Option<HashMap<String, Guard>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UiConfig {
    pub port: u16,
    pub open_on_startup: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            port: 9091,
            open_on_startup: true,
        }
    }
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OTelConfig {
    pub host: String,
    pub port: u16,
}

impl Default for OTelConfig {
    fn default() -> Self {
        Self {
            host: "[::]".to_string(),
            port: 4317,
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

            if let Some(open_ui_on_startup) = &args.open_ui_on_startup {
                self.ui.open_on_startup = *open_ui_on_startup;
            }

            if let Some(cors) = &args.cors_origins {
                self.http.cors_allowed_origins =
                    cors.split(',').map(|s| s.trim().to_string()).collect();
            }

            if let Some(otel_port) = args.otel_port {
                self.otel.port = otel_port;
            }
        }
        self
    }
}
