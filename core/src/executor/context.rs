use crate::model::ModelMetadataFactory;
use crate::types::guardrails::service::GuardrailsEvaluator;
use crate::{
    error::GatewayError,
    handler::{extract_tags, CallbackHandlerFn},
    types::{credentials::Credentials, gateway::CostCalculator},
};
use actix_web::{HttpMessage, HttpRequest};
use std::{collections::HashMap, sync::Arc};

use super::ProvidersConfig;
use crate::routing::interceptor::InterceptorFactory;
use crate::routing::interceptor::RouterInterceptorFactory;

#[derive(Clone)]
pub struct ExecutorContext {
    pub callbackhandler: CallbackHandlerFn,
    pub cost_calculator: Arc<Box<dyn CostCalculator>>,
    pub tags: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub key_credentials: Option<Credentials>,
    pub providers_config: Option<ProvidersConfig>,
    pub evaluator_service: Arc<Box<dyn GuardrailsEvaluator>>,
    pub model_metadata_factory: Arc<Box<dyn ModelMetadataFactory>>,
}

// Implement Send + Sync since all fields are Send + Sync
unsafe impl Send for ExecutorContext {}
unsafe impl Sync for ExecutorContext {}

impl ExecutorContext {
    pub fn new(
        callbackhandler: CallbackHandlerFn,
        cost_calculator: Arc<Box<dyn CostCalculator>>,
        model_metadata_factory: Arc<Box<dyn ModelMetadataFactory>>,
        req: &HttpRequest,
        evaluator_service: Arc<Box<dyn GuardrailsEvaluator>>,
    ) -> Result<Self, GatewayError> {
        let tags = extract_tags(req)?;
        let headers = req
            .headers()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let key_credentials = req.extensions().get::<Credentials>().cloned();
        let providers_config = req.app_data::<ProvidersConfig>().cloned();

        Ok(Self {
            callbackhandler,
            cost_calculator,
            model_metadata_factory,
            tags,
            headers,
            key_credentials,
            providers_config,
            evaluator_service,
        })
    }

    pub fn get_interceptor_factory(&self) -> Box<dyn InterceptorFactory> {
        Box::new(RouterInterceptorFactory::new(self.clone()))
    }
}
