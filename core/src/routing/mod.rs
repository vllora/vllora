use crate::handler::AvailableModels;
use crate::routing::strategy::script::ScriptStrategy;
use crate::types::gateway::ChatCompletionRequest;
use std::collections::HashMap;
use thiserror::Error;

pub mod strategy;

#[derive(Error, Debug)]
pub enum RouterError {
    #[error("Routing script error: {0}")]
    ScriptError(String),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct LlmRouter {
    pub name: String,
    pub strategy: RoutingStrategy,
    pub models: Vec<String>,
    pub max_cost: Option<f64>,
    pub fallbacks: Option<Vec<String>>,
}

/// Defines the primary optimization strategy for model selection
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutingStrategy {
    Cost {
        max_cost_per_million_tokens: Option<f64>,
        willingness_to_pay: Option<f64>,
    },
    Latency,
    #[default]
    Time,
    Random,
    ABTesting {
        model_a: (String, f64),
        model_b: (String, f64),
    },
    Transformed {
        parameters: serde_json::Value,
    },
    Script {
        script: String,
        // js function. It gets parameters in parameters
        // transform_request({request, models, metrics}) -> request
    },
    MinMax(String),
}

#[async_trait::async_trait]
pub trait RouteStrategy {
    async fn route(
        &self,
        mut request: ChatCompletionRequest,
        available_models: AvailableModels,
        headers: HashMap<String, String>,
    ) -> Result<ChatCompletionRequest, RouterError>;
}

#[async_trait::async_trait]
impl RouteStrategy for LlmRouter {
    async fn route(
        &self,
        request: ChatCompletionRequest,
        available_models: AvailableModels,
        headers: HashMap<String, String>,
    ) -> Result<ChatCompletionRequest, RouterError> {
        match &self.strategy {
            RoutingStrategy::Cost { .. } => {
                Ok(request.with_model("openai/gpt-3.5-turbo".to_string()))
            }
            RoutingStrategy::Latency => {
                // Route to the fastest model
                Ok(request.with_model("openai/gpt-3.5-turbo".to_string()))
            }
            RoutingStrategy::Time => {
                // Time-based routing (e.g., different models at different times)
                Ok(request.with_model("openai/gpt-4o-mini".to_string()))
            }
            RoutingStrategy::Random => {
                // Randomly select between available models
                use rand::Rng;

                let mut rng = rand::thread_rng();
                let idx = rng.gen_range(0..self.models.len());
                Ok(request.with_model(self.models[idx].clone()))
            }
            RoutingStrategy::ABTesting {
                model_a: (model_a, split_a),
                model_b: (model_b, spilt_b),
            } => {
                // A/B testing between models based on ModelPairWithSplit
                let rand_val = rand::random::<f64>() * (split_a + spilt_b);
                if rand_val < *split_a {
                    Ok(request.with_model(model_a.clone()))
                } else {
                    Ok(request.with_model(model_b.clone()))
                }
            }
            RoutingStrategy::Transformed { .. } => {
                // Execute a script to transform the request
                // Ok(request.with_model(parameters.clone()))
                Ok(request)
            }
            RoutingStrategy::Script { script } => {
                // Execute a script to transform the request
                Ok(request.with_model(ScriptStrategy::run(script, headers, available_models)?))
            }
            RoutingStrategy::MinMax(_metric) => {
                // Ok(request.with_model(model_name.clone()))
                Ok(request)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LlmRouter;

    #[test]
    fn test_router() {
        let router = LlmRouter {
            name: "test".to_string(),
            strategy: super::RoutingStrategy::ABTesting {
                model_a: ("gpt-3.5-turbo-0125".to_string(), 0.25),
                model_b: ("gpt-4o-mini".to_string(), 0.75),
            },
            models: vec!["gpt-3.5-turbo-0125".to_string(), "gpt-4o-mini".to_string()],
            max_cost: None,
            fallbacks: None,
        };
        println!("{:?}", serde_json::to_string(&router).unwrap());
    }
}
