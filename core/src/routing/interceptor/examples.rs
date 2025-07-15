use super::{Interceptor, InterceptorContext, InterceptorError};
use crate::types::gateway::ChatCompletionRequest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

/// Example interceptor that logs request details
pub struct LoggingInterceptor {
    name: String,
    log_level: String,
}

impl LoggingInterceptor {
    pub fn new(name: String, log_level: String) -> Self {
        Self { name, log_level }
    }
}

#[async_trait::async_trait]
impl Interceptor for LoggingInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn pre_request(&self, context: &mut InterceptorContext) -> Result<serde_json::Value, InterceptorError> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let log_data = serde_json::json!({
            "timestamp": timestamp,
            "model": context.request.model,
            "message_count": context.request.messages.len(),
            "level": self.log_level
        });

        match self.log_level.as_str() {
            "debug" => tracing::debug!("Pre-request: {}", log_data),
            "info" => tracing::info!("Pre-request: {}", log_data),
            "warn" => tracing::warn!("Pre-request: {}", log_data),
            "error" => tracing::error!("Pre-request: {}", log_data),
            _ => tracing::info!("Pre-request: {}", log_data),
        }

        Ok(log_data)
    }

    async fn post_request(&self, context: &mut InterceptorContext, response: &serde_json::Value) -> Result<serde_json::Value, InterceptorError> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let log_data = serde_json::json!({
            "timestamp": timestamp,
            "model": context.request.model,
            "response_available": !response.is_null(),
            "level": self.log_level
        });

        match self.log_level.as_str() {
            "debug" => tracing::debug!("Post-request: {}", log_data),
            "info" => tracing::info!("Post-request: {}", log_data),
            "warn" => tracing::warn!("Post-request: {}", log_data),
            "error" => tracing::error!("Post-request: {}", log_data),
            _ => tracing::info!("Post-request: {}", log_data),
        }

        Ok(log_data)
    }
}

/// Example interceptor that tracks request metrics
pub struct MetricsInterceptor {
    name: String,
    track_tokens: bool,
    track_latency: bool,
}

impl MetricsInterceptor {
    pub fn new(name: String, track_tokens: bool, track_latency: bool) -> Self {
        Self {
            name,
            track_tokens,
            track_latency,
        }
    }
}

#[async_trait::async_trait]
impl Interceptor for MetricsInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn pre_request(&self, context: &mut InterceptorContext) -> Result<serde_json::Value, InterceptorError> {
        let start_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let mut metrics = serde_json::json!({
            "start_time": start_time,
            "model": context.request.model,
            "request_id": context.request.model.clone() + &start_time.to_string()
        });

        if self.track_tokens {
            // Estimate token count (this is a simplified estimation)
            let estimated_tokens: usize = context.request.messages
                .iter()
                .map(|msg| msg.content.len() / 4) // Rough estimation: 4 chars per token
                .sum();
            
            metrics["estimated_input_tokens"] = serde_json::Value::Number(estimated_tokens.into());
        }

        if self.track_latency {
            metrics["start_timestamp"] = serde_json::Value::Number(start_time.into());
        }

        Ok(metrics)
    }

    async fn post_request(&self, context: &mut InterceptorContext, response: &serde_json::Value) -> Result<serde_json::Value, InterceptorError> {
        let end_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let mut metrics = serde_json::json!({
            "end_time": end_time,
            "model": context.request.model,
            "response_received": !response.is_null()
        });

        if self.track_latency {
            // Get start time from pre-request interceptor state
            let state = context.state.read().await;
            if let Some(pre_data) = state.get_pre_request_data(&self.name) {
                if let Some(start_time) = pre_data.get("start_timestamp") {
                    if let Some(start_val) = start_time.as_u64() {
                        let latency = end_time - start_val as u128;
                        metrics["latency_ms"] = serde_json::Value::Number(latency.into());
                    }
                }
            }
        }

        if self.track_tokens {
            // This would typically parse the actual response to count tokens
            metrics["response_processing"] = serde_json::Value::Bool(true);
        }

        Ok(metrics)
    }
}

/// Example interceptor that validates and transforms requests
pub struct ValidationInterceptor {
    name: String,
    max_tokens: Option<u32>,
    allowed_models: Vec<String>,
    required_headers: Vec<String>,
}

impl ValidationInterceptor {
    pub fn new(
        name: String,
        max_tokens: Option<u32>,
        allowed_models: Vec<String>,
        required_headers: Vec<String>,
    ) -> Self {
        Self {
            name,
            max_tokens,
            allowed_models,
            required_headers,
        }
    }
}

#[async_trait::async_trait]
impl Interceptor for ValidationInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn pre_request(&self, context: &mut InterceptorContext) -> Result<serde_json::Value, InterceptorError> {
        let mut validation_result = serde_json::json!({
            "validated": true,
            "model": context.request.model,
            "validations": []
        });

        let mut validations = Vec::new();

        // Validate model
        if !self.allowed_models.is_empty() && !self.allowed_models.contains(&context.request.model) {
            return Err(InterceptorError::ValidationError(format!(
                "Model '{}' not in allowed models: {:?}",
                context.request.model, self.allowed_models
            )));
        }
        validations.push("model_allowed");

        // Validate max tokens
        if let Some(max_tokens) = self.max_tokens {
            if let Some(request_tokens) = context.request.max_tokens {
                if request_tokens > max_tokens {
                    return Err(InterceptorError::ValidationError(format!(
                        "Requested tokens {} exceeds maximum {}",
                        request_tokens, max_tokens
                    )));
                }
            }
        }
        validations.push("tokens_within_limit");

        // Validate required headers
        for header in &self.required_headers {
            if !context.headers.contains_key(header) {
                return Err(InterceptorError::ValidationError(format!(
                    "Required header '{}' missing",
                    header
                )));
            }
        }
        validations.push("headers_present");

        validation_result["validations"] = serde_json::Value::Array(
            validations
                .into_iter()
                .map(|v| serde_json::Value::String(v.to_string()))
                .collect()
        );

        Ok(validation_result)
    }

    async fn post_request(&self, context: &mut InterceptorContext, response: &serde_json::Value) -> Result<serde_json::Value, InterceptorError> {
        let post_validation = serde_json::json!({
            "model": context.request.model,
            "response_validated": true,
            "response_size": response.to_string().len()
        });

        Ok(post_validation)
    }
}

/// Example interceptor that caches responses
pub struct CachingInterceptor {
    name: String,
    cache: std::sync::Arc<tokio::sync::RwLock<HashMap<String, serde_json::Value>>>,
    ttl_seconds: u64,
}

impl CachingInterceptor {
    pub fn new(name: String, ttl_seconds: u64) -> Self {
        Self {
            name,
            cache: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            ttl_seconds,
        }
    }

    fn generate_cache_key(&self, request: &ChatCompletionRequest) -> String {
        // Simple cache key generation - in practice, you'd want a more sophisticated approach
        format!("{}_{}", request.model, serde_json::to_string(&request.messages).unwrap_or_default())
    }
}

#[async_trait::async_trait]
impl Interceptor for CachingInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn pre_request(&self, context: &mut InterceptorContext) -> Result<serde_json::Value, InterceptorError> {
        let cache_key = self.generate_cache_key(&context.request);
        let cache = self.cache.read().await;
        
        let cache_result = if let Some(cached_response) = cache.get(&cache_key) {
            serde_json::json!({
                "cache_key": cache_key,
                "cache_hit": true,
                "cached_response": cached_response.clone()
            })
        } else {
            serde_json::json!({
                "cache_key": cache_key,
                "cache_hit": false
            })
        };

        Ok(cache_result)
    }

    async fn post_request(&self, context: &mut InterceptorContext, response: &serde_json::Value) -> Result<serde_json::Value, InterceptorError> {
        let cache_key = self.generate_cache_key(&context.request);
        
        // Store response in cache
        let mut cache = self.cache.write().await;
        cache.insert(cache_key.clone(), response.clone());

        let cache_result = serde_json::json!({
            "cache_key": cache_key,
            "cached": true,
            "ttl_seconds": self.ttl_seconds
        });

        Ok(cache_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::interceptor::{InterceptorState, InterceptorContext};
    use crate::types::gateway::{ChatCompletionRequest, ChatCompletionMessage};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_logging_interceptor() {
        let interceptor = LoggingInterceptor::new("test_logger".to_string(), "info".to_string());
        
        let state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatCompletionMessage::new_text("user".to_string(), "Hello".to_string())],
            ..Default::default()
        };
        let headers = HashMap::new();

        let mut context = InterceptorContext::new(request, headers, state);
        
        let result = interceptor.pre_request(&mut context).await;
        assert!(result.is_ok());
        
        let data = result.unwrap();
        assert_eq!(data["model"], "gpt-4");
        assert_eq!(data["message_count"], 1);
    }

    #[tokio::test]
    async fn test_validation_interceptor() {
        let interceptor = ValidationInterceptor::new(
            "test_validator".to_string(),
            Some(100),
            vec!["gpt-4".to_string(), "gpt-3.5-turbo".to_string()],
            vec!["authorization".to_string()],
        );

        let state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatCompletionMessage::new_text("user".to_string(), "Hello".to_string())],
            max_tokens: Some(50),
            ..Default::default()
        };
        let mut headers = HashMap::new();
        headers.insert("authorization".to_string(), "Bearer token".to_string());

        let mut context = InterceptorContext::new(request, headers, state);
        
        let result = interceptor.pre_request(&mut context).await;
        assert!(result.is_ok());
        
        let data = result.unwrap();
        assert_eq!(data["validated"], true);
        assert_eq!(data["model"], "gpt-4");
    }

    #[tokio::test]
    async fn test_validation_interceptor_failure() {
        let interceptor = ValidationInterceptor::new(
            "test_validator".to_string(),
            Some(100),
            vec!["gpt-3.5-turbo".to_string()],
            vec![],
        );

        let state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatCompletionMessage::new_text("user".to_string(), "Hello".to_string())],
            ..Default::default()
        };
        let headers = HashMap::new();

        let mut context = InterceptorContext::new(request, headers, state);
        
        let result = interceptor.pre_request(&mut context).await;
        assert!(result.is_err());
        
        match result.unwrap_err() {
            InterceptorError::ValidationError(msg) => {
                assert!(msg.contains("not in allowed models"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }
}