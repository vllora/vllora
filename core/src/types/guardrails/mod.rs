use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub mod evaluator;
pub mod service;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardModel {
    #[serde(rename = "model", default = "default_model_guardrails")]
    pub model: String,
    #[serde(rename = "system_prompt")]
    pub system_prompt: Option<String>,
    #[serde(rename = "user_prompt_template")]
    pub user_prompt_template: String,
}
fn default_model_guardrails() -> String {
    "gpt-4o".to_string()
}

#[derive(Debug, Error)]
pub enum GuardError {
    #[error("Guard not found: {0}")]
    GuardNotFound(String),

    #[error("Guard evaluation error: {0}")]
    GuardEvaluationError(String),

    #[error("Output guardrails not supported in streaming")]
    OutputGuardrailsNotSupportedInStreaming,

    #[error("Request stopped after guard evaluation: {0}")]
    RequestStoppedAfterGuardEvaluation(String),
}

/// Enum representing when a guard should be applied
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GuardStage {
    /// Applied to user messages before being sent to the LLM
    Input,
    /// Applied to LLM responses before being returned to the user
    Output,
}

/// Enum representing what action a guard should take
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GuardAction {
    /// Only observes and logs results without blocking
    Observe,
    /// Validates and can block/fail a request
    Validate,
}

/// The result of a guard evaluation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GuardResult {
    /// Pass/fail result
    Boolean {
        passed: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    /// Text result for observation
    Text {
        text: String,
        passed: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    /// Structured JSON result
    Json { schema: Value, passed: bool },
}

/// Base guard configuration shared by all guard types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct GuardConfig {
    pub id: String,
    pub name: String,
    pub template_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub stage: GuardStage,
    pub action: GuardAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_defined_parameters: Option<Value>,
}
/// The main Guard type that encompasses all guard types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Guard {
    /// Schema-based guard using JSON schema for validation
    Schema {
        #[serde(flatten)]
        config: GuardConfig,
        user_defined_schema: Value,
    },
    /// LLM-based guard that uses another LLM as a judge
    LlmJudge {
        #[serde(flatten)]
        config: GuardConfig,
        model: Option<GuardModel>,
    },
    /// Dataset-based guard that uses vector similarity to examples
    Dataset {
        #[serde(flatten)]
        config: GuardConfig,
        embedding_model: String,
        threshold: f64,
        dataset: DatasetSource,
        schema: Value,
    },
    /// Regex-based guard that validates text against regex patterns
    Regex {
        #[serde(flatten)]
        config: GuardConfig,
        parameters: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum DatasetSource {
    /// A dataset of examples without labels
    Examples(Vec<GuardExample>),
    /// A dataset name that will be loaded from a source
    Source(String),
}

/// Example entry for dataset-based guard
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub struct GuardExample {
    pub text: String,
    pub label: bool,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardTemplate {
    pub name: String,
    pub description: String,
    pub r#type: String,
    pub tags: Vec<String>,
    pub parameters: Value,
}

/// Trait for loading datasets
#[async_trait::async_trait]
pub trait DatasetLoader: Send + Sync {
    async fn load(&self, source: &str) -> Result<Vec<GuardExample>, String>;
}

impl Guard {
    /// Returns the stage at which this guard should be applied
    pub fn stage(&self) -> &GuardStage {
        match self {
            Guard::Schema { config, .. } => &config.stage,
            Guard::LlmJudge { config, .. } => &config.stage,
            Guard::Dataset { config, .. } => &config.stage,
            Guard::Regex { config, .. } => &config.stage,
        }
    }

    /// Returns the action this guard should take
    pub fn action(&self) -> &GuardAction {
        match self {
            Guard::Schema { config, .. } => &config.action,
            Guard::LlmJudge { config, .. } => &config.action,
            Guard::Dataset { config, .. } => &config.action,
            Guard::Regex { config, .. } => &config.action,
        }
    }

    /// Returns the ID of this guard
    pub fn id(&self) -> &String {
        match self {
            Guard::Schema { config, .. } => &config.id,
            Guard::LlmJudge { config, .. } => &config.id,
            Guard::Dataset { config, .. } => &config.id,
            Guard::Regex { config, .. } => &config.id,
        }
    }

    /// Returns the name of this guard
    pub fn name(&self) -> &String {
        match self {
            Guard::Schema { config, .. } => &config.name,
            Guard::LlmJudge { config, .. } => &config.name,
            Guard::Dataset { config, .. } => &config.name,
            Guard::Regex { config, .. } => &config.name,
        }
    }
    pub fn parameters(&self) -> Option<&Value> {
        match self {
            Guard::Schema { config, .. } => config.user_defined_parameters.as_ref(),
            Guard::LlmJudge { config, .. } => config.user_defined_parameters.as_ref(),
            Guard::Dataset { config, .. } => config.user_defined_parameters.as_ref(),
            Guard::Regex { config, .. } => config.user_defined_parameters.as_ref(),
        }
    }
    pub fn set_parameters(&mut self, parameters: Value) {
        match self {
            Guard::Schema { config, .. } => config.user_defined_parameters = Some(parameters),
            Guard::LlmJudge { config, .. } => config.user_defined_parameters = Some(parameters),
            Guard::Dataset { config, .. } => config.user_defined_parameters = Some(parameters),
            Guard::Regex { config, .. } => config.user_defined_parameters = Some(parameters),
        }
    }

    pub fn termplate_id(&self) -> &String {
        match self {
            Guard::Schema { config, .. } => &config.template_id,
            Guard::LlmJudge { config, .. } => &config.template_id,
            Guard::Dataset { config, .. } => &config.template_id,
            Guard::Regex { config, .. } => &config.template_id,
        }
    }
}
