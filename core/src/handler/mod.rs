pub mod chat;
pub mod embedding;
pub mod events;
pub mod image;
pub mod middleware;
pub mod models;
pub mod providers;
pub mod responses;
pub mod runs;
pub mod spans;
pub mod threads;
pub mod traces;

use crate::model::types::ModelEvent;
use crate::models::ModelMetadata;
use crate::types::engine::Model;
use crate::types::metadata::services::model::ModelService;
use crate::GatewayApiError;
use crate::{error::GatewayError, model::error::ModelError};
use actix_web::HttpRequest;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

/// Macro to convert a Result<T, E> into Result<HttpResponse>
///
/// Takes an expression that returns a Result, maps the Ok value to an HttpResponse::Ok().json(),
/// and wraps the entire result in Ok().
///
/// // expands to:
/// // Ok(service.get_data().map(|data| HttpResponse::Ok().json(data))?)
/// ```
#[macro_export]
macro_rules! ok_json {
    ($expr:expr) => {
        Ok($expr.map(|result| actix_web::HttpResponse::Ok().json(result))?)
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableModels(pub Vec<ModelMetadata>);

pub fn find_model_by_full_name(
    model_name: &str,
    model_service: &dyn ModelService,
) -> Result<ModelMetadata, GatewayApiError> {
    let model_parts = model_name.split('/').collect::<Vec<&str>>();
    let llm_model = if model_parts.len() == 1 {
        model_service
            .get_by_name(model_name, None)
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?
            .first()
            .cloned()
    } else if model_parts.len() == 2 {
        let model_name = model_parts.last().expect("2 elements in model parts");
        let provided_by = model_parts.first().expect("2 elements in model parts");

        let model_parts = model_name.split('@').collect::<Vec<&str>>();
        let model_name = model_parts.first().expect("1 element in model parts");

        model_service
            .get_by_provider_and_name(
                &model_name.to_lowercase(),
                &provided_by.to_lowercase(),
                None,
            )
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?
    } else {
        None
    };

    match llm_model {
        Some(model) => Ok(model.into()),
        None => Err(GatewayApiError::GatewayError(GatewayError::ModelError(
            Box::new(ModelError::ModelNotFound(model_name.to_string())),
        ))),
    }
}

// extract vllora-tags from headers, shoule be sth like this: tag1=value1&tag2=value2 => result should be a Map<String, String>
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
                    tag.split_once('=')
                        .map_or((tag.to_string(), "-".to_string()), |(k, v)| {
                            (k.to_string(), v.to_string())
                        })
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
    pub model: Option<Model>,
}

impl ModelEventWithDetails {
    pub fn new(event: ModelEvent, model: Option<Model>) -> Self {
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
    async fn can_execute_llm(&mut self) -> Result<bool, Box<dyn std::error::Error>>;
    async fn get_usage(&self) -> Result<DollarUsage, Box<dyn std::error::Error>>;
}

#[derive(Clone)]
pub struct LimitCheckWrapper {
    pub checkers: Vec<Arc<Mutex<dyn LimitCheck>>>,
}

impl LimitCheckWrapper {
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn can_execute_llm(&self) -> Result<bool, Box<dyn std::error::Error>> {
        for checker in &self.checkers {
            let mut checker = checker.lock().await;

            if !checker.can_execute_llm().await? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub async fn get_usage(&self) -> Result<DollarUsage, Box<dyn std::error::Error>> {
        let first_checker = self
            .checkers
            .first()
            .expect("At least one checker is defined");
        let checker = first_checker.lock().await;
        checker.get_usage().await
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
    #[tracing::instrument(level = "debug", skip(self))]
    async fn can_execute_llm(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(true)
    }

    async fn get_usage(&self) -> Result<DollarUsage, Box<dyn std::error::Error>> {
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
            .can_execute_llm()
            .await
            .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;
        if !can_execute {
            return Err(GatewayApiError::TokenUsageLimit);
        }
    }

    Ok(())
}
