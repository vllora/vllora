use crate::routing::interceptor::types::{
    LimitEntity, LimitTarget, RateLimitAction, RateLimitPeriod, RateLimiter,
};
use crate::routing::interceptor::{Interceptor, InterceptorContext, InterceptorError};
use crate::types::gateway::ChatCompletionRequest;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};

/// State for tracking rate limit usage
#[derive(Debug, Clone)]
pub struct RateLimitState {
    pub current_usage: u64,
    pub last_reset: Instant,
    pub window_start: Instant,
}

impl RateLimitState {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            current_usage: 0,
            last_reset: now,
            window_start: now,
        }
    }

    pub fn reset_if_needed(&mut self, period: &RateLimitPeriod) {
        let now = Instant::now();
        let period_duration = period.to_duration();
        
        if now.duration_since(self.window_start) >= period_duration {
            self.current_usage = 0;
            self.window_start = now;
            self.last_reset = now;
        }
    }

    pub fn increment(&mut self, amount: u64) {
        self.current_usage += amount;
    }

    pub fn is_limit_exceeded(&self, limit: u64) -> bool {
        self.current_usage >= limit
    }
}

impl RateLimitPeriod {
    pub fn to_duration(&self) -> Duration {
        match self {
            RateLimitPeriod::Minute => Duration::from_secs(60),
            RateLimitPeriod::Hour => Duration::from_secs(3600),
            RateLimitPeriod::Day => Duration::from_secs(86400),
            RateLimitPeriod::Month => Duration::from_secs(2592000), // 30 days
            RateLimitPeriod::Year => Duration::from_secs(31536000), // 365 days
        }
    }
}

/// Rate limiter interceptor implementation
pub struct RateLimiterInterceptor {
    config: RateLimiter,
    state: Arc<RwLock<HashMap<String, RateLimitState>>>,
}

impl RateLimiterInterceptor {
    pub fn new(config: RateLimiter) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the entity key for rate limiting
    fn get_entity_key(&self, request: &ChatCompletionRequest, headers: &HashMap<String, String>) -> String {
        match &self.config.limit_entity {
            LimitEntity::UserName => {
                request.user.clone().unwrap_or_else(|| "anonymous".to_string())
            }
            LimitEntity::UserId => {
                // Extract from headers or request
                headers.get("x-user-id")
                    .cloned()
                    .unwrap_or_else(|| "anonymous".to_string())
            }
            LimitEntity::ProjectId => {
                headers.get("x-project-id")
                    .cloned()
                    .unwrap_or_else(|| "default".to_string())
            }
            LimitEntity::OrganizationId => {
                headers.get("x-organization-id")
                    .cloned()
                    .unwrap_or_else(|| "default".to_string())
            }
            LimitEntity::Model => {
                request.model.clone()
            }
            LimitEntity::Provider => {
                // Extract provider from model name
                request.model.split('/').next().unwrap_or("unknown").to_string()
            }
            LimitEntity::Custom(key) => {
                headers.get(key)
                    .cloned()
                    .unwrap_or_else(|| "default".to_string())
            }
        }
    }

    /// Calculate the usage amount for this request
    fn calculate_usage(&self, request: &ChatCompletionRequest) -> u64 {
        match &self.config.limit_target {
            LimitTarget::InputTokens => {
                // Estimate input tokens based on message content
                request.messages.iter()
                    .map(|msg| {
                        msg.content.as_ref()
                            .map(|content| content.as_string().unwrap_or_default().len() / 4) // Rough estimate
                            .unwrap_or(0)
                    })
                    .sum::<usize>() as u64
            }
            LimitTarget::OutputTokens => {
                // For pre-request, we can't know output tokens yet
                // This would be better handled in post-request
                0
            }
            LimitTarget::Requests => 1,
            LimitTarget::Cost => {
                // Estimate cost based on model and input tokens
                let input_tokens = self.calculate_usage(&ChatCompletionRequest {
                    model: request.model.clone(),
                    messages: request.messages.clone(),
                    ..Default::default()
                });
                
                // Rough cost estimation (this should be replaced with actual pricing)
                match request.model.as_str() {
                    m if m.contains("gpt-4") => input_tokens * 30, // $0.03 per 1K tokens
                    m if m.contains("gpt-3.5") => input_tokens * 2, // $0.002 per 1K tokens
                    _ => input_tokens * 10, // Default estimate
                }
            }
            LimitTarget::Custom(_) => 1, // Default to 1 for custom targets
        }
    }

    /// Check if the request should be blocked
    async fn check_rate_limit(&self, entity_key: &str, usage: u64) -> Result<bool, InterceptorError> {
        let mut state_map = self.state.write().await;
        
        let state = state_map.entry(entity_key.to_string()).or_insert_with(RateLimitState::new);
        state.reset_if_needed(&self.config.period);
        
        if state.is_limit_exceeded(self.config.limit) {
            return Ok(false); // Limit exceeded
        }
        
        state.increment(usage);
        Ok(true) // Within limit
    }
}

#[async_trait::async_trait]
impl Interceptor for RateLimiterInterceptor {
    fn name(&self) -> &str {
        "rate_limiter"
    }

    async fn pre_request(
        &self,
        context: &mut InterceptorContext,
    ) -> Result<Value, InterceptorError> {
        let entity_key = self.get_entity_key(&context.request, &context.headers);
        let usage = self.calculate_usage(&context.request);
        
        let within_limit = self.check_rate_limit(&entity_key, usage).await?;
        
        if !within_limit {
            match &self.config.action {
                Some(RateLimitAction::Block) => {
                    return Err(InterceptorError::ExecutionError(
                        "Rate limit exceeded".to_string()
                    ));
                }
                Some(RateLimitAction::Throttle) => {
                    // In a real implementation, you might add a delay here
                    tracing::warn!("Rate limit exceeded, throttling request");
                }
                Some(RateLimitAction::Redirect(model)) => {
                    // Update the request to use the redirected model
                    context.request.model = model.clone();
                }
                Some(RateLimitAction::Fallback(model)) => {
                    // Update the request to use the fallback model
                    context.request.model = model.clone();
                }
                None => {
                    // Default to blocking
                    return Err(InterceptorError::ExecutionError(
                        "Rate limit exceeded".to_string()
                    ));
                }
            }
        }
        
        // Return rate limit information
        Ok(serde_json::json!({
            "entity_key": entity_key,
            "usage": usage,
            "limit": self.config.limit,
            "within_limit": within_limit,
            "period": format!("{:?}", self.config.period),
            "target": format!("{:?}", self.config.limit_target),
        }))
    }

    async fn post_request(
        &self,
        _context: &mut InterceptorContext,
        _response: &Value,
    ) -> Result<Value, InterceptorError> {
        // For post-request, we could update usage based on actual response
        // For now, just return success
        Ok(serde_json::json!({
            "status": "completed",
            "rate_limit_applied": true,
        }))
    }
}

/// Factory for creating rate limiter interceptors
pub struct RateLimiterFactory;

impl RateLimiterFactory {
    pub fn create(config: RateLimiter) -> RateLimiterInterceptor {
        RateLimiterInterceptor::new(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::gateway::{ChatCompletionMessage, ChatCompletionContent};

    fn create_test_request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "openai/gpt-4".to_string(),
            messages: vec![
                ChatCompletionMessage {
                    role: "user".to_string(),
                    content: Some(ChatCompletionContent::Text("Hello, world!".to_string())),
                    tool_calls: None,
                    refusal: None,
                    tool_call_id: None,
                }
            ],
            user: Some("test_user".to_string()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let config = RateLimiter::new(
            10,
            LimitTarget::Requests,
            LimitEntity::UserName,
            RateLimitPeriod::Minute,
        );
        
        let rate_limiter = RateLimiterInterceptor::new(config);
        let mut headers = HashMap::new();
        headers.insert("x-user-id".to_string(), "test_user".to_string());
        
        let state = Arc::new(RwLock::new(HashMap::new()));
        let mut context = InterceptorContext {
            request: create_test_request(),
            headers,
            state,
            metadata: HashMap::new(),
        };
        
        // First request should succeed
        let result = rate_limiter.pre_request(&mut context).await;
        assert!(result.is_ok());
        
        let result_value = result.unwrap();
        assert_eq!(result_value["within_limit"], true);
    }

    #[test]
    fn test_rate_limit_period_duration() {
        assert_eq!(RateLimitPeriod::Minute.to_duration(), Duration::from_secs(60));
        assert_eq!(RateLimitPeriod::Hour.to_duration(), Duration::from_secs(3600));
        assert_eq!(RateLimitPeriod::Day.to_duration(), Duration::from_secs(86400));
    }

    #[test]
    fn test_entity_key_extraction() {
        let config = RateLimiter::new(
            100,
            LimitTarget::Requests,
            LimitEntity::UserName,
            RateLimitPeriod::Hour,
        );
        
        let rate_limiter = RateLimiterInterceptor::new(config);
        let request = create_test_request();
        let mut headers = HashMap::new();
        
        // Test UserName entity
        let key = rate_limiter.get_entity_key(&request, &headers);
        assert_eq!(key, "test_user");
        
        // Test UserId entity
        let config = RateLimiter::new(
            100,
            LimitTarget::Requests,
            LimitEntity::UserId,
            RateLimitPeriod::Hour,
        );
        let rate_limiter = RateLimiterInterceptor::new(config);
        headers.insert("x-user-id".to_string(), "user123".to_string());
        let key = rate_limiter.get_entity_key(&request, &headers);
        assert_eq!(key, "user123");
    }
}