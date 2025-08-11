use crate::routing::interceptor::{Interceptor, InterceptorContext, InterceptorError};
use crate::routing::{LimitEntity, LimitTarget};
use crate::usage::LimitPeriod;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

/// Actions to take when rate limit is exceeded
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitAction {
    Block,
    Throttle,
    Log,
    Redirect(String),
}

/// Configuration for rate limiting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiterConfig {
    pub limit: f64,
    pub limit_target: LimitTarget,
    pub limit_entity: LimitEntity,
    pub period: LimitPeriod,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burst_protection: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<RateLimitAction>,
}

/// Rate limit state for tracking usage
#[derive(Debug, Clone)]
pub struct RateLimitState {
    pub current_usage: u64,
    pub window_start: Instant,
    pub last_reset: Instant,
}

/// Result of a rate limit check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub current_usage: f64,
    pub limit: f64,
    pub remaining: f64,
}

/// Trait for rate limiter service implementations
#[async_trait::async_trait]
pub trait RateLimiterService: Send + Sync {
    /// Check if a request is allowed based on rate limiting rules
    async fn check_rate_limit(
        &self,
        entity_id: &str,
        config: &RateLimiterConfig,
    ) -> Result<RateLimitResult, InterceptorError>;
}

/// In-memory rate limiter service implementation
pub struct InMemoryRateLimiterService {}

impl Default for InMemoryRateLimiterService {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRateLimiterService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl RateLimiterService for InMemoryRateLimiterService {
    async fn check_rate_limit(
        &self,
        _entity_id: &str,
        _config: &RateLimiterConfig,
    ) -> Result<RateLimitResult, InterceptorError> {
        unimplemented!()
    }
}

/// Main RateLimiter struct that wraps a rate limiter service
pub struct RateLimiter {
    inner: Arc<dyn RateLimiterService>,
    config: RateLimiterConfig,
}

impl RateLimiter {
    /// Create a new RateLimiter with the given service and configuration
    pub fn new(service: Arc<dyn RateLimiterService>, config: RateLimiterConfig) -> Self {
        Self {
            inner: service,
            config,
        }
    }

    /// Check if a request is allowed
    pub async fn check_rate_limit(
        &self,
        entity_id: &str,
    ) -> Result<RateLimitResult, InterceptorError> {
        self.inner.check_rate_limit(entity_id, &self.config).await
    }

    /// Get the rate limit configuration
    pub fn config(&self) -> &RateLimiterConfig {
        &self.config
    }
}

#[async_trait::async_trait]
impl Interceptor for RateLimiter {
    fn name(&self) -> &str {
        "rate_limiter"
    }

    async fn pre_request(
        &self,
        context: &mut InterceptorContext,
    ) -> Result<serde_json::Value, InterceptorError> {
        // Extract entity ID from context (this would need to be implemented based on your needs)
        let entity_id = self.extract_entity_id(context)?;

        // Check if request is allowed
        let result = self.check_rate_limit(&entity_id).await?;

        // Return rate limit information
        Ok(serde_json::json!({
            "entity_id": entity_id,
            "current_usage": result.current_usage,
            "limit": result.limit,
            "remaining": result.remaining,
            "allowed": result.allowed,
        }))
    }

    async fn post_request(
        &self,
        _context: &mut InterceptorContext,
        _response: &serde_json::Value,
    ) -> Result<serde_json::Value, InterceptorError> {
        // Post-request processing could include logging, metrics, etc.
        Ok(serde_json::json!({
            "rate_limiter": "post_request_completed"
        }))
    }

    fn validate_config(&self) -> Result<(), InterceptorError> {
        if self.config.limit <= 0.0 {
            return Err(InterceptorError::ValidationError(
                "Rate limit must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

impl RateLimiter {
    /// Extract entity ID from context based on configuration
    fn extract_entity_id(&self, context: &InterceptorContext) -> Result<String, InterceptorError> {
        match &self.config.limit_entity {
            LimitEntity::UserId => context
                .extra
                .as_ref()
                .and_then(|extra| extra.user.as_ref())
                .and_then(|user| user.id.as_ref())
                .cloned()
                .ok_or_else(|| {
                    InterceptorError::ExecutionError("User ID not found in headers".to_string())
                }),
            LimitEntity::UserTier => context
                .extra
                .as_ref()
                .and_then(|extra| extra.user.as_ref())
                .and_then(|user| user.tiers.as_ref())
                .and_then(|tiers| tiers.first().cloned())
                .ok_or_else(|| {
                    InterceptorError::ExecutionError("User tier not found in headers".to_string())
                }),
        }
    }
}

// Helper methods for enums
impl LimitTarget {
    pub fn get_name(&self) -> &str {
        match self {
            LimitTarget::Requests => "requests",
            LimitTarget::Cost => "cost",
        }
    }
}

impl LimitPeriod {
    pub fn get_name(&self) -> &str {
        match self {
            LimitPeriod::Hour => "hour",
            LimitPeriod::Day => "day",
            LimitPeriod::Month => "month",
            LimitPeriod::Total => "total",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_config_validation() {
        let config = RateLimiterConfig {
            limit: 0.0, // Invalid limit
            limit_target: LimitTarget::Requests,
            limit_entity: LimitEntity::UserId,
            period: LimitPeriod::Hour,
            burst_protection: None,
            action: None,
        };

        let rate_limiter = RateLimiter::new(Arc::new(InMemoryRateLimiterService::new()), config);
        assert!(rate_limiter.validate_config().is_err());
    }
}
