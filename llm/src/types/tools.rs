use std::collections::HashMap;

use crate::error::LLMResult;
use crate::types::gateway::FunctionParameters;
use serde::{Deserialize, Serialize};

#[async_trait::async_trait]
pub trait Tool: Send + Sync + 'static {
    fn name(&self) -> String;
    fn description(&self) -> String;
    fn get_function_parameters(&self) -> Option<FunctionParameters>;
    async fn run(
        &self,
        input: HashMap<String, serde_json::Value>,
        tags: HashMap<String, String>,
    ) -> LLMResult<serde_json::Value>;
    fn stop_at_call(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ModelTool {
    pub name: String,
    pub description: Option<String>,
    pub passed_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(transparent)]
pub struct ModelTools(pub Vec<ModelTool>);
impl ModelTools {
    pub fn contains(&self, r: &String) -> bool {
        self.0.iter().any(|tool| &tool.name == r)
    }

    pub fn names(&self) -> impl Iterator<Item = &'_ String> {
        self.0.iter().map(|tool| &tool.name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ModelTool> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<ModelTool> for ModelTools {
    fn from_iter<T: IntoIterator<Item = ModelTool>>(iter: T) -> Self {
        Self(Vec::from_iter(iter))
    }
}
