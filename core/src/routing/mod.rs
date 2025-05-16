// use crate::routing::strategy::script::ScriptError;
// use crate::routing::strategy::script::ScriptStrategy;
use crate::types::gateway::ChatCompletionRequest;
use crate::{handler::AvailableModels, usage::ProviderMetrics};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use thiserror::Error;

pub mod strategy;

#[derive(Error, Debug)]
pub enum RouterError {
    // #[error(transparent)]
    // ScriptError(#[from] ScriptError),
    #[error("Unknown metric for routing: {0}")]
    UnkwownMetric(String),

    #[error("Failed serializing script router result to request: {0}")]
    FailedToDeserializeRequestResult(#[from] serde_json::Error),

    #[error("Metric router error: {0}")]
    MetricRouterError(String),

    #[error("Transformation router error: {0}")]
    TransformationRouterError(String),

    #[error("Invalid metric: {0}")]
    InvalidMetric(String),

    #[error(transparent)]
    BoxedError(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[error("Target by index not found: {0}")]
    TargetByIndexNotFound(usize),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum MetricsDuration {
    Total,
    Last15Minutes,
    LastHour,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct LlmRouter {
    pub name: String,
    #[serde(flatten)]
    pub strategy: RoutingStrategy,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub targets: Vec<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub metrics_duration: Option<MetricsDuration>,
}

/// Defines the primary optimization strategy for model selection
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutingStrategy {
    Fallback,
    #[serde(alias = "a_b_testing")]
    Percentage {
        targets_percentages: Vec<f64>,
    },
    Random,
    // Script {
    //     script: String,
    //     // js function. Context is passed in parameters
    //     // transform_request({request, models, metrics, headers}) -> request
    // },
    Optimized {
        metric: strategy::metric::MetricSelector,
    },
}

impl Display for RoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingStrategy::Fallback => write!(f, "Fallback"),
            RoutingStrategy::Percentage { .. } => write!(f, "Percentage"),
            RoutingStrategy::Random => write!(f, "Random"),
            RoutingStrategy::Optimized { .. } => write!(f, "Optimized"),
        }
    }
}

impl Default for RoutingStrategy {
    fn default() -> Self {
        Self::Optimized {
            metric: strategy::metric::MetricSelector::default(),
        }
    }
}

pub type Target = HashMap<String, serde_json::Value>;

pub type Targets = Vec<Target>;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TargetOrRouterName {
    String(String),
    Target(Target),
}

#[async_trait::async_trait]
pub trait RouteStrategy {
    async fn route(
        &self,
        request: ChatCompletionRequest,
        available_models: &AvailableModels,
        headers: HashMap<String, String>,
        metrics: BTreeMap<String, ProviderMetrics>,
    ) -> Result<Targets, RouterError>;
}

#[async_trait::async_trait]
impl RouteStrategy for LlmRouter {
    async fn route(
        &self,
        _request: ChatCompletionRequest,
        _available_models: &AvailableModels,
        _headers: HashMap<String, String>,
        metrics: BTreeMap<String, ProviderMetrics>,
    ) -> Result<Targets, RouterError> {
        match &self.strategy {
            RoutingStrategy::Fallback => Ok(self.targets.clone()),
            RoutingStrategy::Random => {
                // Randomly select between available models
                use rand::Rng;

                let mut rng = rand::thread_rng();
                let idx = rng.gen_range(0..self.targets.len());
                Ok(vec![self.targets[idx].clone()])
            }
            RoutingStrategy::Percentage {
                targets_percentages,
            } => {
                // it should be 100, but it is not restricted
                let total_percentages: f64 = targets_percentages.iter().sum();
                // A/B testing between models based on ModelPairWithSplit
                let rand_val = rand::random::<f64>() * total_percentages;

                let mut sum = 0.0;
                let idx = targets_percentages
                    .iter()
                    .position(|x| {
                        let prev_sum = sum;
                        sum += x;
                        rand_val >= prev_sum && rand_val < sum
                    })
                    .unwrap_or(0);

                let target = match self.targets.get(idx) {
                    Some(target) => target.clone(),
                    None => return Err(RouterError::TargetByIndexNotFound(idx)),
                };

                Ok(vec![target])
            }
            // RoutingStrategy::Script { script } => {
            //     let result =
            //         ScriptStrategy::run(script, &request, &headers, available_models, &metrics)?;

            //     let r = serde_json::from_value(result)
            //         .map_err(RouterError::FailedToDeserializeRequestResult)?;

            //     Ok(vec![r])
            // }
            RoutingStrategy::Optimized { metric } => {
                let models = self
                    .targets
                    .iter()
                    .filter_map(|m| {
                        m.get("model")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    })
                    .collect::<Vec<_>>();
                let model = strategy::metric::route(
                    &models,
                    &metrics,
                    metric,
                    self.metrics_duration.as_ref(),
                )
                .await?;

                Ok(vec![HashMap::from([(
                    "model".to_string(),
                    serde_json::Value::String(model),
                )])])
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_serialize() {
        let router = LlmRouter {
            name: "dynamic".to_string(),
            strategy: RoutingStrategy::Optimized {
                metric: strategy::metric::MetricSelector::Ttft,
            },
            targets: vec![],
            metrics_duration: None,
        };

        eprintln!("{}", serde_json::to_string_pretty(&router).unwrap());

        let router = LlmRouter {
            name: "dynamic".to_string(),
            strategy: RoutingStrategy::Percentage {
                targets_percentages: vec![0.5, 0.5],
            },
            targets: vec![
                HashMap::from([
                    (
                        "model".to_string(),
                        serde_json::Value::String("openai/gpt-4o-mini".to_string()),
                    ),
                    (
                        "frequence_penality".to_string(),
                        serde_json::Value::Number(1.into()),
                    ),
                ]),
                HashMap::from([
                    (
                        "model".to_string(),
                        serde_json::Value::String("openai/gpt-4o-mini".to_string()),
                    ),
                    (
                        "frequence_penality".to_string(),
                        serde_json::Value::Number(2.into()),
                    ),
                ]),
            ],
            metrics_duration: None,
        };

        eprintln!("{}", serde_json::to_string_pretty(&router).unwrap());
    }

    // #[tokio::test]
    // async fn test_script_router() {
    //     let router = LlmRouter {
    //         name: "test".to_string(),
    //         strategy: RoutingStrategy::Script {
    //             script: r#"
    //                 function route(params) {
    //                     const { request, models, metrics } = params;
    //                     if (request.messages.length > 5) {
    //                         return { ...request, model: "openai/gpt-4" };
    //                     }
    //                     return { ...request, model: "openai/gpt-3.5-turbo" };
    //                 }
    //             "#
    //             .to_string(),
    //         },
    //         targets: vec![],
    //         metrics_duration: None,
    //     };

    //     // Test case 1: Short conversation (≤ 5 messages)
    //     let request = ChatCompletionRequest {
    //         model: "router/test".to_string(),
    //         messages: vec![
    //             ChatCompletionMessage::new_text("user".to_string(), "Hello".to_string()),
    //             ChatCompletionMessage::new_text("assistant".to_string(), "Hi there!".to_string()),
    //         ],
    //         ..Default::default()
    //     };

    //     let headers = HashMap::new();
    //     let available_models = AvailableModels(vec![]);
    //     let metrics = BTreeMap::new();

    //     let result = router
    //         .route(
    //             request.clone(),
    //             &available_models,
    //             headers.clone(),
    //             metrics.clone(),
    //         )
    //         .await;

    //     assert!(result.is_ok());
    //     assert_eq!(
    //         result
    //             .unwrap()
    //             .first()
    //             .expect("No targets")
    //             .get("model")
    //             .expect("No model")
    //             .as_str()
    //             .expect("No model string")
    //             .to_string(),
    //         "openai/gpt-3.5-turbo"
    //     );

    //     // Test case 2: Long conversation (> 5 messages)
    //     let long_request = ChatCompletionRequest {
    //         model: "router/test".to_string(),
    //         messages: vec![
    //             ChatCompletionMessage::new_text("user".to_string(), "Message 1".to_string()),
    //             ChatCompletionMessage::new_text("assistant".to_string(), "Response 1".to_string()),
    //             ChatCompletionMessage::new_text("user".to_string(), "Message 2".to_string()),
    //             ChatCompletionMessage::new_text("assistant".to_string(), "Response 2".to_string()),
    //             ChatCompletionMessage::new_text("user".to_string(), "Message 3".to_string()),
    //             ChatCompletionMessage::new_text("assistant".to_string(), "Response 3".to_string()),
    //         ],
    //         ..Default::default()
    //     };

    //     let result = router
    //         .route(long_request, &available_models, headers, metrics)
    //         .await;

    //     assert!(result.is_ok());
    //     assert_eq!(
    //         result
    //             .unwrap()
    //             .first()
    //             .expect("No targets")
    //             .get("model")
    //             .expect("No model")
    //             .as_str()
    //             .expect("No model string")
    //             .to_string(),
    //         "openai/gpt-4"
    //     );

    //     // Test serialization
    //     let serialized = serde_json::to_string_pretty(&router).unwrap();
    //     let deserialized: LlmRouter = serde_json::from_str(&serialized).unwrap();
    //     assert_eq!(router.name, deserialized.name);
    //     assert_eq!(router.targets, deserialized.targets);
    //     match (&router.strategy, &deserialized.strategy) {
    //         (RoutingStrategy::Script { script: s1 }, RoutingStrategy::Script { script: s2 }) => {
    //             assert_eq!(s1, s2);
    //         }
    //         _ => panic!("Deserialized strategy does not match"),
    //     }
    // }
}
