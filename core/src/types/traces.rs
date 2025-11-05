use rmcp::schemars;
use serde::Serialize;
use serde::{Deserialize, Deserializer, Serializer};
use serde_json::Value;
use std::{collections::HashMap, fmt::Display};

#[derive(Serialize, schemars::JsonSchema)]
pub struct LangdbSpan {
    pub trace_id: String,
    pub span_id: String,
    pub thread_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub operation_name: Operation,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub attribute: HashMap<String, Value>,
    pub child_attribute: Option<HashMap<String, Value>>,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, schemars::JsonSchema)]
#[schemars(
    description = "The operation name. Available operations: run, agent, task, tools, openai, anthropic, bedrock, gemini, cloud_api_invoke, api_invoke, model_call"
)]
pub enum Operation {
    Run,
    CloudApiInvoke,
    ApiInvoke,
    ModelCall,
    Agent,
    Task,
    Tools,
    Openai,
    Anthropic,
    Bedrock,
    Gemini,
    Other(String),
}

impl Serialize for Operation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s: String = self.clone().into();
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for Operation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Operation::from(s.as_str()))
    }
}

impl From<&str> for Operation {
    fn from(value: &str) -> Self {
        match value {
            "run" => Operation::Run,
            "agent" => Operation::Agent,
            "task" => Operation::Task,
            "tools" => Operation::Tools,
            "openai" => Operation::Openai,
            "anthropic" => Operation::Anthropic,
            "bedrock" => Operation::Bedrock,
            "gemini" => Operation::Gemini,
            "cloud_api_invoke" => Operation::CloudApiInvoke,
            "api_invoke" => Operation::ApiInvoke,
            "model_call" => Operation::ModelCall,
            other => Operation::Other(other.to_string()),
        }
    }
}

impl From<Operation> for String {
    fn from(value: Operation) -> Self {
        match value {
            Operation::Run => "run".to_string(),
            Operation::Agent => "agent".to_string(),
            Operation::Task => "task".to_string(),
            Operation::Tools => "tools".to_string(),
            Operation::Openai => "openai".to_string(),
            Operation::Anthropic => "anthropic".to_string(),
            Operation::Bedrock => "bedrock".to_string(),
            Operation::Gemini => "gemini".to_string(),
            Operation::CloudApiInvoke => "cloud_api_invoke".to_string(),
            Operation::ApiInvoke => "api_invoke".to_string(),
            Operation::ModelCall => "model_call".to_string(),
            Operation::Other(other) => other,
        }
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operation::Run => write!(f, "run"),
            Operation::Agent => write!(f, "agent"),
            Operation::Task => write!(f, "task"),
            Operation::Tools => write!(f, "tools"),
            Operation::Openai => write!(f, "openai"),
            Operation::Anthropic => write!(f, "anthropic"),
            Operation::Bedrock => write!(f, "bedrock"),
            Operation::Gemini => write!(f, "gemini"),
            Operation::CloudApiInvoke => write!(f, "cloud_api_invoke"),
            Operation::ApiInvoke => write!(f, "api_invoke"),
            Operation::ModelCall => write!(f, "model_call"),
            Operation::Other(other) => write!(f, "{other}"),
        }
    }
}
