pub mod chat;
pub mod embedding;
pub mod image;
pub mod middleware;
pub mod models;

use crate::error::GatewayError;
use crate::model::types::ModelEvent;
use crate::models::ModelDefinition;
use crate::types::engine::Model;
use crate::GatewayApiError;
use actix_web::HttpRequest;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct AvailableModels(pub Vec<ModelDefinition>);

pub fn find_model_by_full_name(
    model_name: &str,
    provided_models: &AvailableModels,
) -> Result<ModelDefinition, GatewayApiError> {
    let model_parts = model_name.split('/').collect::<Vec<&str>>();

    let llm_model = if model_parts.len() == 1 {
        provided_models
            .0
            .iter()
            .find(|m| m.model.to_lowercase() == model_name.to_lowercase())
            .cloned()
    } else if model_parts.len() == 2 {
        let model_name = model_parts.last().expect("2 elements in model parts");
        let provided_by = model_parts.first().expect("2 elements in model parts");

        provided_models
            .0
            .iter()
            .find(|m| {
                (m.model.to_lowercase() == model_name.to_lowercase()
                    || m.inference_provider.model_name == model_name.to_lowercase())
                    && m.inference_provider.provider.to_string() == *provided_by
            })
            .cloned()
    } else {
        None
    };

    match llm_model {
        Some(model) => Ok(model),
        None => Err(GatewayApiError::CustomError(format!(
            "Model not found {model_name}"
        ))),
    }
}

// extract langdb-tags from headers, shoule be sth like this: tag1=value1&tag2=value2 => result should be a Map<String, String>
pub fn extract_tags(req: &HttpRequest) -> Result<HashMap<String, String>, GatewayError> {
    Ok(match req.headers().get("x-tags") {
        Some(value) => {
            let tags_str = value
                .to_str()
                .map_err(|e| GatewayError::CustomError(e.to_string()))?
                .to_string();
            let tags: HashMap<String, String> = tags_str
                .split('&')
                .map(|tag| {
                    let (k, v) = tag.split_once('=').unwrap();
                    (k.to_string(), v.to_string())
                })
                .collect();
            Some(tags)
        }
        None => None,
    }
    .unwrap_or_default())
}

pub fn record_map_err(
    e: impl Into<GatewayApiError> + ToString,
    span: tracing::Span,
) -> GatewayApiError {
    span.record("error", e.to_string());
    e.into()
}

#[derive(Clone, Default)]
pub struct CallbackHandlerFn(pub Option<tokio::sync::broadcast::Sender<ModelEventWithDetails>>);

impl CallbackHandlerFn {
    pub fn on_message(&self, message: ModelEventWithDetails) {
        if let Some(sender) = self.0.clone() {
            let _ = sender.send(message);
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModelEventWithDetails {
    pub event: ModelEvent,
    pub model: Model,
}

impl ModelEventWithDetails {
    pub fn new(event: ModelEvent, model: Model) -> Self {
        Self { event, model }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DollarUsage {
    pub daily: f64,
    pub daily_limit: Option<f64>,
    pub monthly: f64,
    pub monthly_limit: Option<f64>,
    pub total: f64,
    pub total_limit: Option<f64>,
}

#[async_trait::async_trait]
pub trait LimitCheck {
    async fn can_execute_llm(
        &mut self,
        tenant_name: &str,
        project_id: &str,
    ) -> Result<bool, Box<dyn std::error::Error>>;
    async fn get_usage(
        &self,
        tenant_name: &str,
        project_id: &str,
    ) -> Result<DollarUsage, Box<dyn std::error::Error>>;
}

#[derive(Clone)]
pub struct LimitCheckWrapper {
    pub checkers: Vec<Arc<Mutex<dyn LimitCheck>>>,
}

impl LimitCheckWrapper {
    pub async fn can_execute_llm(
        &self,
        tenant_name: &str,
        project_id: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        for checker in &self.checkers {
            let mut checker = checker.lock().await;

            if !checker.can_execute_llm(tenant_name, project_id).await? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub async fn get_usage(
        &self,
        tenant_name: &str,
        project_id: &str,
    ) -> Result<DollarUsage, Box<dyn std::error::Error>> {
        let first_checker = self
            .checkers
            .first()
            .expect("At least one checker is defined");
        let checker = first_checker.lock().await;
        checker.get_usage(tenant_name, project_id).await
    }
}

impl Default for LimitCheckWrapper {
    fn default() -> Self {
        Self {
            checkers: vec![Arc::new(Mutex::new(DefaultLimitCheck))],
        }
    }
}
pub struct DefaultLimitCheck;

#[async_trait::async_trait]
impl LimitCheck for DefaultLimitCheck {
    async fn can_execute_llm(
        &mut self,
        _tenant_name: &str,
        _project_id: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(true)
    }

    async fn get_usage(
        &self,
        _tenant_name: &str,
        _project_id: &str,
    ) -> Result<DollarUsage, Box<dyn std::error::Error>> {
        unimplemented!()
    }
}

impl LimitCheckWrapper {
    pub fn new(checkers: Vec<Arc<Mutex<dyn LimitCheck>>>) -> Self {
        Self { checkers }
    }
}

pub(crate) async fn can_execute_llm_for_request(req: &HttpRequest) -> Result<(), GatewayApiError> {
    let limit_checker = req.app_data::<Option<LimitCheckWrapper>>();
    if let Some(Some(l)) = limit_checker {
        let can_execute = l
            .can_execute_llm("default", "")
            .await
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;
        if !can_execute {
            return Err(GatewayApiError::TokenUsageLimit);
        }
    }

    Ok(())
}
