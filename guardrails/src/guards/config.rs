use std::collections::HashMap;

use langdb_core::types::guardrails::{Guard, GuardTemplate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GuardsTemplatesConfig {
    pub templates: HashMap<String, GuardTemplate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GuardsConfig {
    pub guards: HashMap<String, Guard>,
}

/// Load the default guards from the embedded configuration
pub fn load_guard_templates() -> Result<HashMap<String, GuardTemplate>, serde_yaml::Error> {
    let default_config = include_str!("config/templates.yaml");
    let config: GuardsTemplatesConfig = serde_yaml::from_str(default_config)?;
    Ok(config.templates)
}

/// Load guards from a YAML configuration string
pub fn load_guards_from_yaml(yaml_str: &str) -> Result<HashMap<String, Guard>, serde_yaml::Error> {
    let config: GuardsConfig = serde_yaml::from_str(yaml_str)?;
    Ok(config.guards)
}
