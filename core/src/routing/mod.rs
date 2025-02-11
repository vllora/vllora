use crate::routing::strategy::script::ScriptError;
use crate::routing::strategy::script::ScriptStrategy;
use crate::types::gateway::ChatCompletionRequest;
use crate::{handler::AvailableModels, usage::ProviderMetrics};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

pub mod strategy;

#[derive(Error, Debug)]
pub enum RouterError {
    #[error(transparent)]
    ScriptError(#[from] ScriptError),

    #[error("Unknown metric for routing: {0}")]
    UnkwownMetric(String),

    #[error("Failed serializing script router result to request: {0}")]
    FailedToDeserializeRequestResult(#[from] serde_json::Error),

    #[error("Metric router error: {0}")]
    MetricRouterError(String),

    #[error("Transformation router error: {0}")]
    TransformationRouterError(String),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct LlmRouter {
    pub name: String,
    pub strategy: RoutingStrategy,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
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
    Percentage {
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
    Min(String),
}

#[async_trait::async_trait]
pub trait RouteStrategy {
    async fn route(
        &self,
        request: ChatCompletionRequest,
        available_models: AvailableModels,
        headers: HashMap<String, String>,
        metrics: BTreeMap<String, ProviderMetrics>,
    ) -> Result<ChatCompletionRequest, RouterError>;
}

#[async_trait::async_trait]
impl RouteStrategy for LlmRouter {
    async fn route(
        &self,
        request: ChatCompletionRequest,
        available_models: AvailableModels,
        headers: HashMap<String, String>,
        metrics: BTreeMap<String, ProviderMetrics>,
    ) -> Result<ChatCompletionRequest, RouterError> {
        match &self.strategy {
            RoutingStrategy::Cost { .. } => {
                unimplemented!()
            }
            RoutingStrategy::Latency => {
                strategy::metric::route(
                    &self.models,
                    request,
                    available_models,
                    headers,
                    &metrics,
                    strategy::metric::MetricSelector::Ttft,
                    true,
                )
                .await
            }
            RoutingStrategy::Time => {
                strategy::metric::route(
                    &self.models,
                    request,
                    available_models,
                    headers,
                    &metrics,
                    strategy::metric::MetricSelector::RequestsDuration,
                    true,
                )
                .await
            }
            RoutingStrategy::Random => {
                // Randomly select between available models
                use rand::Rng;

                let mut rng = rand::thread_rng();
                let idx = rng.gen_range(0..self.models.len());
                Ok(request.with_model(self.models[idx].clone()))
            }
            RoutingStrategy::Percentage {
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
            RoutingStrategy::Transformed { parameters } => {
                // Execute a script to transform the request
                let params = parameters.as_object().ok_or_else(|| {
                    RouterError::TransformationRouterError(
                        "parameters must be an object".to_string(),
                    )
                })?;

                // Convert request to Value, merge with parameters, and convert back
                let mut request_value = serde_json::to_value(&request)
                    .map_err(RouterError::FailedToDeserializeRequestResult)?;

                if let Some(obj) = request_value.as_object_mut() {
                    for (key, value) in params {
                        // Only override if the new value is not null
                        if !value.is_null() {
                            obj.insert(key.clone(), value.clone());
                        }
                    }
                }

                serde_json::from_value(request_value)
                    .map_err(RouterError::FailedToDeserializeRequestResult)
            }
            RoutingStrategy::Script { script } => {
                let result =
                    ScriptStrategy::run(script, &request, &headers, &available_models, &metrics)?;

                Ok(serde_json::from_value(result)
                    .map_err(RouterError::FailedToDeserializeRequestResult)?)
            }
            RoutingStrategy::Min(metric) => {
                // Use metric-based routing with the specified metric
                let metric_selector = match metric.as_str() {
                    "requests" => strategy::metric::MetricSelector::Requests,
                    "input_tokens" => strategy::metric::MetricSelector::InputTokens,
                    "output_tokens" => strategy::metric::MetricSelector::OutputTokens,
                    "total_tokens" => strategy::metric::MetricSelector::TotalTokens,
                    "requests_duration" => strategy::metric::MetricSelector::RequestsDuration,
                    "ttft" => strategy::metric::MetricSelector::Ttft,
                    "llm_usage" => strategy::metric::MetricSelector::LlmUsage,
                    _ => return Err(RouterError::UnkwownMetric(metric.clone())),
                };

                strategy::metric::route(
                    &self.models,
                    request,
                    available_models,
                    headers,
                    &metrics,
                    metric_selector,
                    true,
                )
                .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::types::gateway::ChatCompletionMessage;

    use super::*;

    #[tokio::test]
    async fn test_script_router() {
        let router = LlmRouter {
            name: "test".to_string(),
            strategy: RoutingStrategy::Script {
                script: r#"
                    function route(params) {
                        const { request, models, metrics } = params;
                        if (request.messages.length > 5) {
                            return { ...request, model: "openai/gpt-4" };
                        }
                        return { ...request, model: "openai/gpt-3.5-turbo" };
                    }
                "#
                .to_string(),
            },
            models: vec![
                "openai/gpt-3.5-turbo".to_string(),
                "openai/gpt-4".to_string(),
            ],
            fallback: None,
            max_cost: None,
        };

        // Test case 1: Short conversation (≤ 5 messages)
        let request = ChatCompletionRequest {
            model: "router/test".to_string(),
            messages: vec![
                ChatCompletionMessage::new_text("user".to_string(), "Hello".to_string()),
                ChatCompletionMessage::new_text("assistant".to_string(), "Hi there!".to_string()),
            ],
            ..Default::default()
        };

        let headers = HashMap::new();
        let available_models = AvailableModels(vec![]);
        let metrics = BTreeMap::new();

        let result = router
            .route(
                request.clone(),
                available_models.clone(),
                headers.clone(),
                metrics.clone(),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().model, "openai/gpt-3.5-turbo");

        // Test case 2: Long conversation (> 5 messages)
        let long_request = ChatCompletionRequest {
            model: "router/test".to_string(),
            messages: vec![
                ChatCompletionMessage::new_text("user".to_string(), "Message 1".to_string()),
                ChatCompletionMessage::new_text("assistant".to_string(), "Response 1".to_string()),
                ChatCompletionMessage::new_text("user".to_string(), "Message 2".to_string()),
                ChatCompletionMessage::new_text("assistant".to_string(), "Response 2".to_string()),
                ChatCompletionMessage::new_text("user".to_string(), "Message 3".to_string()),
                ChatCompletionMessage::new_text("assistant".to_string(), "Response 3".to_string()),
            ],
            ..Default::default()
        };

        let result = router
            .route(long_request, available_models, headers, metrics)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().model, "openai/gpt-4");

        // Test serialization
        let serialized = serde_json::to_string_pretty(&router).unwrap();
        let deserialized: LlmRouter = serde_json::from_str(&serialized).unwrap();
        assert_eq!(router.name, deserialized.name);
        assert_eq!(router.models, deserialized.models);
        match (&router.strategy, &deserialized.strategy) {
            (RoutingStrategy::Script { script: s1 }, RoutingStrategy::Script { script: s2 }) => {
                assert_eq!(s1, s2);
            }
            _ => panic!("Deserialized strategy does not match"),
        }
    }

    #[tokio::test]
    async fn test_transformed_router() {
        let router = LlmRouter {
            name: "test".to_string(),
            strategy: RoutingStrategy::Transformed {
                parameters: serde_json::json!({
                    "model": "openai/gpt-4",
                    "temperature": 0.7,
                    "max_tokens": 128,
                    "top_p": 0.9,
                    "presence_penalty": 0.5,
                    "frequency_penalty": 0.3,
                    "stop": ["END", "STOP"]
                }),
            },
            models: vec![
                "openai/gpt-3.5-turbo".to_string(),
                "openai/gpt-4".to_string(),
            ],
            fallback: None,
            max_cost: None,
        };

        let request = ChatCompletionRequest {
            model: "router/test".to_string(),
            messages: vec![ChatCompletionMessage::new_text(
                "user".to_string(),
                "Hello".to_string(),
            )],
            temperature: Some(0.5),
            max_tokens: None,
            ..Default::default()
        };

        let headers = HashMap::new();
        let available_models = AvailableModels(vec![]);
        let metrics = BTreeMap::new();

        let result = router
            .route(
                request.clone(),
                available_models.clone(),
                headers.clone(),
                metrics.clone(),
            )
            .await;

        assert!(result.is_ok());
        let transformed = result.unwrap();
        assert_eq!(transformed.model, "openai/gpt-4");
        assert_eq!(transformed.temperature, Some(0.7));
        assert_eq!(transformed.max_tokens, Some(128));
        assert_eq!(transformed.top_p, Some(0.9));
        assert_eq!(transformed.presence_penalty, Some(0.5));
        assert_eq!(transformed.frequency_penalty, Some(0.3));
        assert_eq!(
            transformed.stop,
            Some(vec!["END".to_string(), "STOP".to_string()])
        );

        // Test invalid parameters
        let invalid_router = LlmRouter {
            name: "test".to_string(),
            strategy: RoutingStrategy::Transformed {
                parameters: serde_json::json!("not an object"),
            },
            models: vec![],
            fallback: None,
            max_cost: None,
        };

        let result = invalid_router
            .route(request, available_models, headers, metrics)
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            RouterError::TransformationRouterError(_) => (),
            err => panic!("Expected InvalidParameters error, got: {}", err),
        }
    }
}
