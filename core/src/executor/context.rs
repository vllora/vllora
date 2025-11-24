use crate::credentials::KeyStorage;
use crate::mcp::McpConfig;
use crate::model::ModelMetadataFactory;
use crate::routing::interceptor::rate_limiter::RateLimiterService;
use crate::types::guardrails::service::GuardrailsEvaluator;
use crate::{
    error::GatewayError,
    handler::{extract_tags, CallbackHandlerFn},
};
use actix_web::HttpRequest;
use std::{collections::HashMap, sync::Arc};
use vllora_llm::types::gateway::CostCalculator;

use super::ProvidersConfig;
use crate::routing::interceptor::InterceptorFactory;
use crate::routing::interceptor::RouterInterceptorFactory;

#[derive(Clone)]
pub struct ExecutorContext {
    pub callbackhandler: CallbackHandlerFn,
    pub cost_calculator: Arc<Box<dyn CostCalculator>>,
    pub tags: HashMap<String, String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub providers_config: Option<ProvidersConfig>,
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
        req: &HttpRequest,
        metadata: HashMap<String, serde_json::Value>,
        evaluator_service: Arc<Box<dyn GuardrailsEvaluator>>,
        rate_limiter_service: Arc<dyn RateLimiterService>,
        project_id: uuid::Uuid,
        key_storage: Arc<Box<dyn KeyStorage>>,
        mcp_config: Option<McpConfig>,
    ) -> Result<Self, GatewayError> {
        let tags = extract_tags(req)?;

        let providers_config = req.app_data::<ProvidersConfig>().cloned();

        Ok(Self {
            callbackhandler,
            cost_calculator,
            model_metadata_factory,
            tags,
            metadata,
            providers_config,
            evaluator_service,
            rate_limiter_service,
            project_id,
            key_storage,
            mcp_config,
        })
    }

    pub fn get_interceptor_factory(&self) -> Box<dyn InterceptorFactory> {
        Box::new(RouterInterceptorFactory::new(self.clone()))
    }
}
