use std::collections::HashMap;

use vllora_llm::error::LLMResult;
use vllora_llm::types::gateway::ChatCompletionTool;

use vllora_llm::types::gateway::FunctionParameters;
use vllora_llm::types::tools::Tool;

pub struct GatewayTool {
    pub def: ChatCompletionTool,
}

#[async_trait::async_trait]
impl Tool for GatewayTool {
    fn name(&self) -> String {
        self.def.function.name.to_string()
    }

    fn description(&self) -> String {
        self.def
            .function
            .description
            .clone()
            .unwrap_or("".to_string())
    }

    fn get_function_parameters(&self) -> std::option::Option<FunctionParameters> {
        self.def
            .function
            .parameters
            .as_ref()
            .map(|p| serde_json::from_value(p.clone()).unwrap())
    }

    async fn run(
        &self,
        _inputs: HashMap<String, serde_json::Value>,
        _tags: HashMap<String, String>,
    ) -> LLMResult<serde_json::Value> {
        panic!("Gateway tool should not be called directly");
    }

    fn stop_at_call(&self) -> bool {
        true
    }
}
