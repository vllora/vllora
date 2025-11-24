use crate::model::ModelMetadataFactory;
use crate::routing::metrics::MetricsRepository;
use vllora_telemetry::events::JsonValue;
// use crate::routing::strategy::script::ScriptError;
// use crate::routing::strategy::script::ScriptStrategy;
use crate::routing::strategy::conditional::ConditionalRouter;
use crate::usage::LimitPeriod;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;
use thiserror::Error;
use valuable::Valuable;
use vllora_llm::types::gateway::{ChatCompletionRequest, Extra};

pub mod interceptor;
pub mod metrics;
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
}

impl LlmRouter {
    pub fn new(name: String, strategy: RoutingStrategy) -> Self {
        Self {
            name,
            strategy,
            targets: Vec::new(),
            metrics_duration: None,
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
}

/// Extended routing result that includes interceptor state
#[derive(Debug, Clone)]
pub struct RoutingResult {
    pub targets: Targets,
}

impl RoutingResult {
    pub fn new(targets: Targets) -> Self {
        Self { targets }
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
    /// Conditional routing based on request or context conditions
    Conditional {
        #[serde(flatten)]
        routing: ConditionalRouting,
    },
}

impl Display for RoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingStrategy::Fallback => write!(f, "Fallback"),
            RoutingStrategy::Percentage { .. } => write!(f, "Percentage"),
            RoutingStrategy::Random => write!(f, "Random"),
            RoutingStrategy::Optimized { .. } => write!(f, "Optimized"),
            RoutingStrategy::Conditional { .. } => write!(f, "Conditional"),
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ConditionalRouting {
    #[serde(default)]
    pub pre_request: Vec<InterceptorSpec>,
    pub routes: Vec<Route>,
    #[serde(default)]
    pub post_request: Vec<InterceptorSpec>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct InterceptorSpec {
    pub name: String,
    #[serde(flatten)]
    pub interceptor_type: InterceptorType,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InterceptorType {
    #[serde(alias = "guard")]
    Guardrail { guard_id: String },
    RateLimiter {
        limit: f64,
        period: LimitPeriod,
        target: LimitTarget,
        entity: LimitEntity,
    },
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LimitEntity {
    #[serde(alias = "user_id")]
    UserId,
    #[serde(alias = "user_tier")]
    UserTier,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LimitTarget {
    Cost,
    Requests,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Route {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<RouteCondition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<TargetSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_mapper: Option<MessageMapper>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum RouteCondition {
    All { all: Vec<ConditionExpr> },
    Any { any: Vec<ConditionExpr> },
    Expr(HashMap<String, ConditionOp>),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum ConditionExpr {
    Expr(HashMap<String, ConditionOp>),
}

impl ConditionExpr {
    /// Validates that all keys in the condition expression match allowed patterns.
    /// Allowed keys:
    /// - "metadata.user.tier"
    /// - "metadata.user.id"
    /// - "metadata.region"
    /// - "pre_request.*.*"
    /// - "metrics.provider.*"
    /// - "metrics.model:*"
    pub fn validate_keys(&self) -> Result<(), String> {
        match self {
            ConditionExpr::Expr(map) => {
                for key in map.keys() {
                    if !Self::is_valid_key(key) {
                        return Err(format!("Invalid condition key: {key}"));
                    }
                }
                Ok(())
            }
        }
    }

    fn is_valid_key(key: &str) -> bool {
        key == "metadata.user.tier"
            || key == "metadata.user.id"
            || key == "metadata.region"
            || key == "metadata.country"
            || key.starts_with("pre_request.")
            || key.starts_with("metrics.provider.")
            || key.starts_with("metrics.model.")
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq)]
pub struct ConditionOp {
    #[serde(flatten)]
    pub op: HashMap<ConditionOpType, serde_json::Value>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ConditionOpType {
    #[serde(alias = "$eq")]
    Eq,
    #[serde(alias = "$ne")]
    Ne,
    #[serde(alias = "$in")]
    In,
    #[serde(alias = "$gt")]
    Gt,
    #[serde(alias = "$lt")]
    Lt,
    #[serde(alias = "$gte")]
    Gte,
    #[serde(alias = "$lte")]
    Lte,
    #[serde(alias = "$contains")]
    Contains,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TargetSort {
    Price,
    #[serde(untagged)]
    Metric(strategy::metric::MetricSelector),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TargetSpec {
    Any {
        #[serde(rename = "$any")]
        any: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", flatten)]
        sort: Option<TargetSortSpec>,
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        filter: HashMap<TargetSort, HashMap<ConditionOpType, serde_json::Value>>,
    },
    List(Vec<HashMap<String, serde_json::Value>>),
    Single(String),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct TargetSortSpec {
    pub sort_by: TargetSort,
    pub sort_order: Option<TargetSortOrder>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum TargetSortOrder {
    Min,
    Max,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct MessageMapper {
    pub modifier: String,
    pub content: String,
}

#[async_trait::async_trait]
pub trait RouteStrategy {
    async fn route<M: MetricsRepository + Send + Sync>(
        &self,
        request: ChatCompletionRequest,
        extra: Option<&Extra>,
        model_metadata_factory: Arc<Box<dyn ModelMetadataFactory>>,
        metadata: HashMap<String, serde_json::Value>,
        metrics_repository: &M,
        interceptor_factory: Box<dyn interceptor::InterceptorFactory>,
    ) -> Result<RoutingResult, RouterError>;
}

#[async_trait::async_trait]
impl RouteStrategy for LlmRouter {
    async fn route<M: MetricsRepository + Send + Sync>(
        &self,
        request: ChatCompletionRequest,
        extra: Option<&Extra>,
        model_metadata_factory: Arc<Box<dyn ModelMetadataFactory>>,
        metadata: HashMap<String, serde_json::Value>,
        metrics_repository: &M,
        interceptor_factory: Box<dyn interceptor::InterceptorFactory>,
    ) -> Result<RoutingResult, RouterError> {
        // Routing logic only, no interceptors
        let targets = match &self.strategy {
            RoutingStrategy::Fallback => self.targets.clone(),
            RoutingStrategy::Random => {
                use rand::Rng;
                let mut rng = rand::rng();
                let idx = rng.random_range(0..self.targets.len());
                vec![self.targets[idx].clone()]
            }
            RoutingStrategy::Percentage {
                targets_percentages,
            } => {
                let total_percentages: f64 = targets_percentages.iter().sum();
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
                    metric,
                    self.metrics_duration.as_ref(),
                    metrics_repository,
                    None,
                    None,
                )
                .await?;
                vec![HashMap::from([(
                    "model".to_string(),
                    serde_json::Value::String(model),
                )])]
            }
            RoutingStrategy::Conditional { routing } => {
                let router = ConditionalRouter {
                    routing: routing.clone(),
                };
                let headers = HashMap::new(); // TODO: pass real headers
                let target_opt = router
                    .get_target(interceptor_factory, &request, &headers, &metadata, extra)
                    .await;

                match target_opt {
                    Some(TargetSpec::List(targets)) => targets.clone(),
                    Some(TargetSpec::Single(model)) => {
                        vec![HashMap::from([(
                            "model".to_string(),
                            serde_json::Value::String(model.clone()),
                        )])]
                    }
                    Some(TargetSpec::Any { any, sort, filter }) => {
                        let model = match sort {
                            Some(TargetSortSpec {
                                sort_by,
                                sort_order,
                            }) => match sort_by {
                                TargetSort::Price => {
                                    let models =
                                        any.iter().map(|m| m.to_string()).collect::<Vec<_>>();
                                    let model = model_metadata_factory
                                        .get_cheapest_model_metadata(&models)
                                        .await
                                        .map_err(|e| {
                                            RouterError::MetricRouterError(e.to_string())
                                        })?;
                                    let span = tracing::Span::current();
                                    span.record(
                                        "router.metric_resolution",
                                        JsonValue(&serde_json::json!({"candidates": [], "best_model": model.qualified_model_name(), "metric": "cost"})).as_value(),
                                    );
                                    model.qualified_model_name()
                                }
                                TargetSort::Metric(metric) => {
                                    let minimize = sort_order
                                        .as_ref()
                                        .map(|s| matches!(s, TargetSortOrder::Min));
                                    let mut filters = HashMap::new();
                                    for (sort, value) in filter {
                                        if let TargetSort::Metric(metric) = sort {
                                            filters.insert(metric.clone(), value.clone());
                                        }
                                    }
                                    strategy::metric::route(
                                        any,
                                        metric,
                                        self.metrics_duration.as_ref(),
                                        metrics_repository,
                                        minimize,
                                        Some(&filters),
                                    )
                                    .await?
                                }
                            },
                            None => any.first().cloned().unwrap_or_default(),
                        };

                        vec![HashMap::from([(
                            "model".to_string(),
                            serde_json::Value::String(model),
                        )])]
                    }
                    None => {
                        return Err(RouterError::MetricRouterError(
                            "No conditional route matched".to_string(),
                        ))
                    }
                }
            }
        };
        Ok(RoutingResult::new(targets))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::DefaultModelMetadataFactory;
    use std::sync::Arc;

    use crate::metadata::services::model::ModelServiceImpl;
    use crate::metadata::test_utils::setup_test_database;
    use crate::routing::interceptor::InterceptorFactory;

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
                        serde_json::Value::String("openai/gpt-4.1-nano".to_string()),
                    ),
                    (
                        "frequence_penality".to_string(),
                        serde_json::Value::Number(1.into()),
                    ),
                ]),
                HashMap::from([
                    (
                        "model".to_string(),
                        serde_json::Value::String("openai/gpt-4.1-nano".to_string()),
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
        };

        // Test routing

        let db_pool = setup_test_database();
        let request = ChatCompletionRequest::default();
        let model_metadata_factory = Arc::new(Box::new(DefaultModelMetadataFactory::new(Arc::new(
            Box::new(ModelServiceImpl::new(db_pool)),
        ))) as Box<dyn ModelMetadataFactory>);
        let headers = HashMap::new();

        struct DummyFactory;
        impl interceptor::InterceptorFactory for DummyFactory {
            fn create_interceptor(
                &self,
                _spec: &InterceptorSpec,
            ) -> Result<Arc<dyn interceptor::Interceptor>, interceptor::InterceptorError>
            {
                Err(interceptor::InterceptorError::ExecutionError(
                    "DummyFactory: no interceptors".to_string(),
                ))
            }
        }
        let dummy_factory = Box::new(DummyFactory) as Box<dyn InterceptorFactory>;
        let result = router
            .route(
                request,
                None,
                model_metadata_factory,
                headers,
                &metrics_repo,
                dummy_factory,
            )
            .await;

        assert!(result.is_ok());
        let routing_result = result.unwrap();
        assert_eq!(routing_result.targets.len(), 1);
        assert_eq!(
            routing_result.targets[0]
                .get("model")
                .unwrap()
                .as_str()
                .unwrap(),
            "openai/gpt-4"
        );
    }

    #[tokio::test]
    async fn test_llm_router_conditional() {
        use crate::metadata::services::model::ModelServiceImpl;
        use crate::model::DefaultModelMetadataFactory;
        use crate::routing::interceptor::{
            Interceptor, InterceptorContext, InterceptorError, InterceptorFactory,
        };
        use crate::routing::metrics::MetricsRepository;
        use crate::routing::{
            ConditionOp, ConditionalRouting, InterceptorSpec, Route, RouteCondition, TargetSpec,
        };
        use std::collections::BTreeMap;
        use std::collections::HashMap;
        use std::sync::Arc;
        use vllora_llm::types::gateway::ChatCompletionRequest;

        struct MockGuardrail {
            result: bool,
        }
        #[async_trait::async_trait]
        impl Interceptor for MockGuardrail {
            fn name(&self) -> &str {
                "guardrail"
            }
            async fn pre_request(
                &self,
                _context: &mut InterceptorContext,
            ) -> Result<serde_json::Value, InterceptorError> {
                Ok(serde_json::json!({"result": self.result}))
            }
            async fn post_request(
                &self,
                _context: &mut InterceptorContext,
                _response: &serde_json::Value,
            ) -> Result<serde_json::Value, InterceptorError> {
                Ok(serde_json::json!({"result": self.result}))
            }
        }
        struct MockFactory {
            result: bool,
        }
        impl InterceptorFactory for MockFactory {
            fn create_interceptor(
                &self,
                spec: &InterceptorSpec,
            ) -> Result<Arc<dyn Interceptor>, InterceptorError> {
                if spec.name == "guardrail" {
                    Ok(Arc::new(MockGuardrail {
                        result: self.result,
                    }))
                } else {
                    Err(InterceptorError::ExecutionError(
                        "Unknown interceptor".to_string(),
                    ))
                }
            }
        }
        struct DummyMetricsRepo;
        #[async_trait::async_trait]
        impl MetricsRepository for DummyMetricsRepo {
            async fn get_metrics(
                &self,
            ) -> Result<BTreeMap<String, crate::usage::ProviderMetrics>, RouterError> {
                Ok(BTreeMap::new())
            }
            async fn get_provider_metrics(
                &self,
                _provider: &str,
            ) -> Result<Option<crate::usage::ProviderMetrics>, RouterError> {
                Ok(Some(crate::usage::ProviderMetrics::default()))
            }
            async fn get_model_metrics(
                &self,
                _provider: &str,
                _model: &str,
            ) -> Result<Option<crate::usage::ModelMetrics>, RouterError> {
                Ok(None)
            }
        }
        // Passing guardrail
        let routing = ConditionalRouting {
            pre_request: vec![InterceptorSpec {
                name: "guardrail".to_string(),
                interceptor_type: InterceptorType::Guardrail {
                    guard_id: "guard_id".to_string(),
                },
                extra: HashMap::new(),
            }],
            routes: vec![Route {
                name: "guarded_route".to_string(),
                conditions: Some(RouteCondition::Expr(HashMap::from([(
                    "pre_request.guardrail.result".to_string(),
                    ConditionOp {
                        op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
                    },
                )]))),
                targets: Some(TargetSpec::List(vec![HashMap::from([(
                    "model".to_string(),
                    serde_json::json!("mock/model"),
                )])])),
                message_mapper: None,
            }],
            post_request: vec![],
        };
        let router = LlmRouter {
            name: "conditional_test".to_string(),
            strategy: RoutingStrategy::Conditional {
                routing: routing.clone(),
            },
            targets: vec![],
            metrics_duration: None,
        };
        let db_pool = setup_test_database();
        let factory = Box::new(MockFactory { result: true }) as Box<dyn InterceptorFactory>;
        let model_metadata_factory = Arc::new(Box::new(DefaultModelMetadataFactory::new(Arc::new(
            Box::new(ModelServiceImpl::new(db_pool.clone())),
        ))) as Box<dyn ModelMetadataFactory>);
        let result = router
            .route(
                ChatCompletionRequest::default(),
                None,
                model_metadata_factory,
                HashMap::new(),
                &DummyMetricsRepo,
                factory,
            )
            .await;
        assert!(result.is_ok());
        let routing_result = result.unwrap();
        assert_eq!(routing_result.targets.len(), 1);
        assert_eq!(routing_result.targets[0]["model"], "mock/model");
        // Failing guardrail
        let factory = Box::new(MockFactory { result: false }) as Box<dyn InterceptorFactory>;
        let model_metadata_factory = Arc::new(Box::new(DefaultModelMetadataFactory::new(Arc::new(
            Box::new(ModelServiceImpl::new(db_pool)),
        ))) as Box<dyn ModelMetadataFactory>);
        let result = router
            .route(
                ChatCompletionRequest::default(),
                None,
                model_metadata_factory,
                HashMap::new(),
                &DummyMetricsRepo,
                factory,
            )
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_route() {
        let route = r#"
        {
                    "name": "toxic",
                    "conditions": {
                        "all": [
                            { "pre_request.toxic.passed": { "eq": false } }
                        ]
                    },
                    "targets": {
                        "$any": ["anthropic/claude-4-opus"],
                        "sort_by": "ttft",
                        "sort_order": "min"
                    },
                    "message_mapper": null  
                }
        "#;
        let route: Route = serde_json::from_str(route).unwrap();

        eprintln!("{}", serde_json::to_string_pretty(&route).unwrap());
    }

    #[test]
    fn test_serialize_route_single() {
        let route = Route {
            name: "basic_user".to_string(),
            conditions: Some(RouteCondition::All { all: vec![] }),
            targets: Some(TargetSpec::Single("anthropic/claude-4-opus".to_string())),
            message_mapper: None,
        };

        let route_str = serde_json::to_string_pretty(&route).unwrap();
        eprintln!("{route_str}");
    }

    #[test]
    fn test_deserialize_conditional_router_with_rate_limiter() {
        let conditional_router = ConditionalRouting {
            pre_request: vec![InterceptorSpec {
                name: "rate_limiter".to_string(),
                interceptor_type: InterceptorType::RateLimiter {
                    limit: 10.0,
                    period: LimitPeriod::Hour,
                    target: LimitTarget::Requests,
                    entity: LimitEntity::UserId,
                },
                extra: HashMap::new(),
            }],
            routes: vec![],
            post_request: vec![],
        };
        let json = serde_json::to_string_pretty(&conditional_router).unwrap();
        eprintln!("{json}");

        let _conditional_router: ConditionalRouting = serde_json::from_str(&json).unwrap();
    }
}
