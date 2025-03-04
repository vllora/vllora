use std::collections::HashMap;

use langdb_core::types::guardrails::Guard;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GuardsConfig {
    pub guards: Vec<Guard>,
}

/// Load guards from a YAML configuration string
pub fn load_guards_from_yaml(yaml_str: &str) -> Result<Vec<Guard>, serde_yaml::Error> {
    let config: GuardsConfig = serde_yaml::from_str(yaml_str)?;
    Ok(config.guards)
}

/// Load the default guards from the embedded configuration
pub fn load_default_guards() -> Result<HashMap<String, Guard>, serde_yaml::Error> {
    let default_config = include_str!("config/default_guards.yaml");
    let guards = load_guards_from_yaml(default_config)?;
    Ok(guards
        .into_iter()
        .map(|g| (g.id.clone(), g.clone()))
        .collect())
}
