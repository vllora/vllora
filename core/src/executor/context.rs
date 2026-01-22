use crate::credentials::KeyStorage;
use crate::handler::CallbackHandlerFn;
use crate::mcp::McpConfig;
use crate::model::ModelMetadataFactory;
use crate::routing::interceptor::rate_limiter::RateLimiterService;
use crate::routing::interceptor::InterceptorFactory;
use crate::routing::interceptor::RouterInterceptorFactory;
use crate::types::guardrails::service::GuardrailsEvaluator;
use std::{collections::HashMap, sync::Arc};
use vllora_llm::types::gateway::CostCalculator;

#[derive(Clone)]
pub struct ExecutorContext {
    pub callbackhandler: CallbackHandlerFn,
    pub cost_calculator: Arc<Box<dyn CostCalculator>>,
    pub tags: HashMap<String, String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub evaluator_service: Arc<Box<dyn GuardrailsEvaluator>>,
    pub model_metadata_factory: Arc<Box<dyn ModelMetadataFactory>>,
    pub rate_limiter_service: Arc<dyn RateLimiterService>,
    pub project_id: uuid::Uuid,
    pub key_storage: Arc<Box<dyn KeyStorage>>,
    pub mcp_config: Option<McpConfig>,
}

// Implement Send + Sync since all fields are Send + Sync
unsafe impl Send for ExecutorContext {}
unsafe impl Sync for ExecutorContext {}

impl ExecutorContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        callbackhandler: CallbackHandlerFn,
        cost_calculator: Arc<Box<dyn CostCalculator>>,
        model_metadata_factory: Arc<Box<dyn ModelMetadataFactory>>,
        tags: HashMap<String, String>,
        metadata: HashMap<String, serde_json::Value>,
        evaluator_service: Arc<Box<dyn GuardrailsEvaluator>>,
        rate_limiter_service: Arc<dyn RateLimiterService>,
        project_id: uuid::Uuid,
        key_storage: Arc<Box<dyn KeyStorage>>,
        mcp_config: Option<McpConfig>,
    ) -> Self {
        Self {
            callbackhandler,
            cost_calculator,
            model_metadata_factory,
            tags,
            metadata,
            evaluator_service,
            rate_limiter_service,
            project_id,
            key_storage,
            mcp_config,
        }
    }

    pub fn get_interceptor_factory(&self) -> Box<dyn InterceptorFactory> {
        Box::new(RouterInterceptorFactory::new(self.clone()))
    }
}
