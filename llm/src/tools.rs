use std::collections::HashMap;

use crate::error::LLMResult;
use crate::mcp::execute_mcp_tool;
use crate::types::gateway::FunctionParameters;
use crate::types::gateway::McpTool;
use crate::types::tools::Tool;

#[async_trait::async_trait]
impl Tool for McpTool {
    fn name(&self) -> String {
        self.0.name.to_string()
    }

    fn description(&self) -> String {
        self.0.description.clone().unwrap_or_default().to_string()
    }

    fn get_function_parameters(&self) -> std::option::Option<FunctionParameters> {
        let schema = self.0.schema_as_json_value();

        serde_json::from_value(schema.clone()).ok()
    }

    async fn run(
        &self,
        inputs: HashMap<String, serde_json::Value>,
        tags: HashMap<String, String>,
    ) -> LLMResult<serde_json::Value> {
        let env = self.1.env();
        let meta = match env {
            Some(env) => serde_json::json!({"env_vars": env}),
            None => serde_json::to_value(tags)?,
        };
        Ok(execute_mcp_tool(&self.1, &self.0, inputs, Some(meta))
            .await
            .map(serde_json::Value::String)?)
    }

    fn stop_at_call(&self) -> bool {
        false
    }
}
