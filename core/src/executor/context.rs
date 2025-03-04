use crate::{
    error::GatewayError,
    handler::{extract_tags, AvailableModels, CallbackHandlerFn},
    types::{credentials::Credentials, gateway::CostCalculator},
    usage::InMemoryStorage,
};
use actix_web::{HttpMessage, HttpRequest};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use super::ProvidersConfig;

#[derive(Clone)]
pub struct ExecutorContext {
    pub callbackhandler: CallbackHandlerFn,
    pub cost_calculator: Arc<Box<dyn CostCalculator>>,
    pub provided_models: AvailableModels,
    pub memory_storage: Option<Arc<Mutex<InMemoryStorage>>>,
    pub tags: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub key_credentials: Option<Credentials>,
    pub providers_config: Option<ProvidersConfig>,
}

impl ExecutorContext {
    pub fn new(
        callbackhandler: CallbackHandlerFn,
        cost_calculator: Arc<Box<dyn CostCalculator>>,
        provided_models: AvailableModels,
        memory_storage: Option<Arc<Mutex<InMemoryStorage>>>,
        req: &HttpRequest,
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
            provided_models,
            memory_storage,
            tags,
            headers,
            key_credentials,
            providers_config,
        })
    }
}
