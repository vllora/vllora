use crate::routing::metrics::MetricsRepository;
// use crate::routing::strategy::script::ScriptError;
// use crate::routing::strategy::script::ScriptStrategy;
use crate::handler::AvailableModels;
use crate::types::gateway::ChatCompletionRequest;
use std::collections::HashMap;
use std::fmt::Display;
use thiserror::Error;

pub mod metrics;
pub mod strategy;
pub mod interceptor;

use interceptor::{InterceptorManager, InterceptorState, InterceptorContext};
use std::sync::Arc;

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

    #[error("Metrics repository error: {0}")]
    MetricsRepositoryError(String),

    #[error("Interceptor error: {0}")]
    InterceptorError(#[from] interceptor::InterceptorError),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub interceptor_manager: Option<Arc<InterceptorManager>>,
}

impl LlmRouter {
    pub fn new(name: String, strategy: RoutingStrategy) -> Self {
        Self {
            name,
            strategy,
            targets: Vec::new(),
            metrics_duration: None,
            interceptor_manager: None,
        }
    }

    pub fn with_targets(mut self, targets: Vec<HashMap<String, serde_json::Value>>) -> Self {
        self.targets = targets;
        self
    }

    pub fn with_metrics_duration(mut self, duration: MetricsDuration) -> Self {
        self.metrics_duration = Some(duration);
        self
    }

    pub fn with_interceptor_manager(mut self, manager: Arc<InterceptorManager>) -> Self {
        self.interceptor_manager = Some(manager);
        self
    }
}

/// Extended routing result that includes interceptor state
#[derive(Debug, Clone)]
pub struct RoutingResult {
    pub targets: Targets,
    pub interceptor_state: Option<Arc<tokio::sync::RwLock<InterceptorState>>>,
}

impl RoutingResult {
    pub fn new(targets: Targets) -> Self {
        Self {
            targets,
            interceptor_state: None,
        }
    }

    pub fn with_interceptor_state(mut self, state: Arc<tokio::sync::RwLock<InterceptorState>>) -> Self {
        self.interceptor_state = Some(state);
        self
    }
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
    async fn route<M: MetricsRepository + Send + Sync>(
        &self,
        request: ChatCompletionRequest,
        available_models: &AvailableModels,
        headers: HashMap<String, String>,
        metrics_repository: &M,
    ) -> Result<RoutingResult, RouterError>;
}

#[async_trait::async_trait]
impl RouteStrategy for LlmRouter {
    async fn route<M: MetricsRepository + Send + Sync>(
        &self,
        request: ChatCompletionRequest,
        available_models: &AvailableModels,
        headers: HashMap<String, String>,
        metrics_repository: &M,
    ) -> Result<RoutingResult, RouterError> {
        // Initialize interceptor state
        let interceptor_state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
        
        // Execute pre-request interceptors if available
        if let Some(interceptor_manager) = &self.interceptor_manager {
            let mut context = InterceptorContext::new(
                request.clone(),
                headers.clone(),
                interceptor_state.clone(),
            );
            
            interceptor_manager.execute_pre_request(&mut context).await?;
            
            tracing::debug!("Pre-request interceptors executed for router: {}", self.name);
        }

        // Execute routing strategy
        let targets = match &self.strategy {
            RoutingStrategy::Fallback => self.targets.clone(),
            RoutingStrategy::Random => {
                // Randomly select between available models
                use rand::Rng;

                let mut rng = rand::rng();
                let idx = rng.random_range(0..self.targets.len());
                vec![self.targets[idx].clone()]
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

                vec![target]
            }
            // RoutingStrategy::Script { script } => {
            //     let result =
            //         ScriptStrategy::run(script, &request, &headers, available_models, &metrics)?;

            //     let r = serde_json::from_value(result)
            //         .map_err(RouterError::FailedToDeserializeRequestResult)?;

            //     vec![r]
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

                // Fetch metrics from the repository
                let metrics = metrics_repository.get_metrics().await?;

                let model = strategy::metric::route(
                    &models,
                    &metrics,
                    metric,
                    self.metrics_duration.as_ref(),
                )
                .await?;

                vec![HashMap::from([(
                    "model".to_string(),
                    serde_json::Value::String(model),
                )])]
            }
        };

        // Execute post-request interceptors if available
        if let Some(interceptor_manager) = &self.interceptor_manager {
            let mut context = InterceptorContext::new(
                request.clone(),
                headers.clone(),
                interceptor_state.clone(),
            );
            
            let response_data = serde_json::json!({
                "targets": targets,
                "router_name": self.name,
                "strategy": self.strategy.to_string()
            });
            
            interceptor_manager.execute_post_request(&mut context, &response_data).await?;
            
            tracing::debug!("Post-request interceptors executed for router: {}", self.name);
        }

        Ok(RoutingResult::new(targets).with_interceptor_state(interceptor_state))
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
            interceptor_manager: None,
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
            interceptor_manager: None,
        };

        eprintln!("{}", serde_json::to_string_pretty(&router).unwrap());
    }

    #[tokio::test]
    async fn test_metrics_repository_integration() {
        use crate::routing::metrics::InMemoryMetricsRepository;
        use crate::usage::ProviderMetrics;
        use crate::usage::{Metrics, ModelMetrics, TimeMetrics};
        use std::collections::BTreeMap;

        // Create sample metrics
        let mut provider_metrics = ProviderMetrics {
            models: BTreeMap::new(),
        };

        // Add model metrics
        provider_metrics.models.insert(
            "gpt-4".to_string(),
            ModelMetrics {
                metrics: TimeMetrics {
                    total: Metrics {
                        requests: Some(100.0),
                        latency: Some(150.0),
                        ttft: Some(50.0),
                        tps: Some(20.0),
                        error_rate: Some(0.01),
                        input_tokens: Some(1000.0),
                        output_tokens: Some(500.0),
                        total_tokens: Some(1500.0),
                        llm_usage: Some(0.5),
                    },
                    last_15_minutes: Metrics::default(),
                    last_hour: Metrics::default(),
                },
            },
        );

        // Create metrics repository
        let mut metrics_map = BTreeMap::new();
        metrics_map.insert("openai".to_string(), provider_metrics);
        let metrics_repo = InMemoryMetricsRepository::new(metrics_map);

        // Create router with optimized strategy
        let router = LlmRouter {
            name: "test_router".to_string(),
            strategy: RoutingStrategy::Optimized {
                metric: strategy::metric::MetricSelector::Latency,
            },
            targets: vec![HashMap::from([(
                "model".to_string(),
                serde_json::Value::String("openai/gpt-4".to_string()),
            )])],
            metrics_duration: Some(MetricsDuration::Total),
            interceptor_manager: None,
        };

        // Test routing
        let request = ChatCompletionRequest::default();
        let available_models = AvailableModels(vec![]);
        let headers = HashMap::new();

        let result = router
            .route(request, &available_models, headers, &metrics_repo)
            .await;

        assert!(result.is_ok());
        let routing_result = result.unwrap();
        assert_eq!(routing_result.targets.len(), 1);
        assert_eq!(
            routing_result.targets[0].get("model").unwrap().as_str().unwrap(),
            "openai/gpt-4"
        );
        assert!(routing_result.interceptor_state.is_none());
    }

    #[tokio::test]
    async fn test_interceptor_integration() {
        use crate::routing::interceptor::{InterceptorManager, Interceptor, InterceptorError};
        use crate::routing::interceptor::InterceptorContext;
        use std::sync::Arc;

        // Create a test interceptor
        struct TestInterceptor;
        
        #[async_trait::async_trait]
        impl Interceptor for TestInterceptor {
            fn name(&self) -> &str {
                "test_interceptor"
            }
            
            async fn pre_request(&self, _context: &mut InterceptorContext) -> Result<serde_json::Value, InterceptorError> {
                Ok(serde_json::json!({"test": "pre_request"}))
            }
            
            async fn post_request(&self, _context: &mut InterceptorContext, _response: &serde_json::Value) -> Result<serde_json::Value, InterceptorError> {
                Ok(serde_json::json!({"test": "post_request"}))
            }
        }

        // Create interceptor manager
        let mut interceptor_manager = InterceptorManager::new();
        interceptor_manager.add_interceptor(Arc::new(TestInterceptor)).unwrap();

        // Create router with interceptor manager
        let router = LlmRouter {
            name: "test_router".to_string(),
            strategy: RoutingStrategy::Optimized {
                metric: strategy::metric::MetricSelector::Latency,
            },
            targets: vec![HashMap::from([(
                "model".to_string(),
                serde_json::Value::String("openai/gpt-4".to_string()),
            )])],
            metrics_duration: Some(MetricsDuration::Total),
            interceptor_manager: Some(Arc::new(interceptor_manager)),
        };

        // Test routing
        let request = ChatCompletionRequest::default();
        let available_models = AvailableModels(vec![]);
        let headers = HashMap::new();

        let result = router
            .route(request, &available_models, headers, &InMemoryMetricsRepository::new(BTreeMap::new()))
            .await;

        assert!(result.is_ok());
        let routing_result = result.unwrap();
        assert_eq!(routing_result.targets.len(), 1);
        assert_eq!(
            routing_result.targets[0].get("model").unwrap().as_str().unwrap(),
            "openai/gpt-4"
        );

        // Check interceptor state exists
        assert!(routing_result.interceptor_state.is_some());
        let state = routing_result.interceptor_state.unwrap();
        let state_read = state.read().await;
        assert_eq!(state_read.pre_request_results.len(), 1);
        assert_eq!(state_read.post_request_results.len(), 1);
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
