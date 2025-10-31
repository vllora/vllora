use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use vllora_core::types::guardrails::{Guard, GuardModel, GuardTemplate};

#[derive(Debug, Serialize, Deserialize)]
pub struct GuardsTemplatesConfig {
    pub templates: HashMap<String, GuardTemplate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GuardsConfig {
    pub guards: HashMap<String, Guard>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GuardsPromptsConfig {
    pub models: HashMap<String, GuardModel>,
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

pub fn load_prompts_from_yaml(
    yaml_str: &str,
) -> Result<HashMap<String, GuardModel>, serde_yaml::Error> {
    let config: GuardsPromptsConfig = serde_yaml::from_str(yaml_str)?;
    Ok(config.models)
}

pub fn default_suffix() -> String {
    r#"
    Return a JSON object with:
      - "text": the analyzed text
      - "passed": boolean - whether the content is factually accurate
      - "confidence": number between 0-1

      Text to analyze: {{text}}
    "#
    .to_string()
}

pub fn default_response_schema() -> serde_json::Value {
    serde_json::json!({
      "type": "object",
      "additionalProperties": false,
      "required": [
        "text",
        "passed",
        "confidence"
      ],
      "properties": {
        "text": {
          "type": "string",
          "description": "The analyzed text"
        },
        "passed": {
          "type": "boolean",
          "description": "Whether the content passed the guard check"
        },
        "confidence": {
          "type": "number",
          // Openai doesnt support
          // "minimum": 0,
          // "maximum": 1,
          "description": "Confidence score of the decision"
        }
      }
    })
}
