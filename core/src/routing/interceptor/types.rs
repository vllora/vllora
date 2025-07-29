use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents different types of interceptors that can be configured
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterceptorType {
    Guardrail {
        guard_id: String,
        config: GuardrailConfig,
    },
    SemanticGuardrail {
        topics: Vec<String>,
        threshold: f64,
        action: GuardrailAction,
    },
    ToxicityGuardrail {
        threshold: f64,
        action: ToxicityAction,
        categories: Vec<String>,
    },
    ComplianceGuardrail {
        regulations: Vec<String>, // GDPR, HIPAA, etc.
        data_classification: String,
        action: ComplianceAction,
    },
    MessageTransformer {
        rules: Vec<TransformRule>,
        direction: TransformDirection, // pre_request, post_response
    },
    MetadataEnricher {
        fields: Vec<String>,
        sources: Vec<MetadataSource>,
    },
    RateLimiter {
        limit: u64,
        limit_target: LimitTarget,
        limit_entity: LimitEntity,
        period: RateLimitPeriod,
        #[serde(skip_serializing_if = "Option::is_none")]
        burst_protection: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<RateLimitAction>,
    },
}

impl InterceptorType {
    /// Check if this interceptor type is allowed in post-request interceptors
    pub fn is_allowed_in_post_request(&self) -> bool {
        matches!(
            self,
            InterceptorType::Guardrail { .. }
                | InterceptorType::SemanticGuardrail { .. }
                | InterceptorType::ToxicityGuardrail { .. }
                | InterceptorType::ComplianceGuardrail { .. }
        )
    }

    /// Get the name/identifier for this interceptor type
    pub fn get_name(&self) -> &str {
        match self {
            InterceptorType::Guardrail { guard_id, .. } => guard_id,
            InterceptorType::SemanticGuardrail { .. } => "semantic_guardrail",
            InterceptorType::ToxicityGuardrail { .. } => "toxicity_guardrail",
            InterceptorType::ComplianceGuardrail { .. } => "compliance_guardrail",
            InterceptorType::MessageTransformer { .. } => "message_transformer",
            InterceptorType::MetadataEnricher { .. } => "metadata_enricher",
            InterceptorType::RateLimiter { .. } => "rate_limiter",
        }
    }
}

/// Configuration for basic guardrails
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<String>>,
}

/// Actions that can be taken by guardrails
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailAction {
    Block,
    Flag,
    Log,
    Transform,
    Redirect(String), // Redirect to different model
}

/// Actions for toxicity guardrails
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToxicityAction {
    Block,
    Flag,
    Filter,
    Sanitize,
}

/// Actions for compliance guardrails
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceAction {
    Block,
    Flag,
    Encrypt,
    Anonymize,
    Log,
}

/// Rules for message transformation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformRule {
    pub pattern: String,
    pub replacement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>, // regex flags
}

/// Direction for message transformation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformDirection {
    PreRequest,
    PostResponse,
    Both,
}

/// Sources for metadata enrichment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataSource {
    User,
    Request,
    Headers,
    Variables,
    External(String), // External API or service
}

/// Rate limiting targets
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitTarget {
    InputTokens,
    OutputTokens,
    Requests,
    Cost,
    Custom(String),
}

/// Rate limiting entities
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitEntity {
    UserName,
    UserId,
    ProjectId,
    OrganizationId,
    Model,
    Provider,
    Custom(String),
}

/// Rate limiting periods
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitPeriod {
    Minute,
    Hour,
    Day,
    Month,
    Year,
}

/// Rate limiting actions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitAction {
    Block,
    Throttle,
    Redirect(String), // Redirect to different model
    Fallback(String), // Use fallback model
}

/// Rate limiter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiter {
    pub limit: u64,
    pub limit_target: LimitTarget,
    pub limit_entity: LimitEntity,
    pub period: RateLimitPeriod,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burst_protection: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<RateLimitAction>,
}

impl RateLimiter {
    pub fn new(
        limit: u64,
        limit_target: LimitTarget,
        limit_entity: LimitEntity,
        period: RateLimitPeriod,
    ) -> Self {
        Self {
            limit,
            limit_target,
            limit_entity,
            period,
            burst_protection: None,
            action: None,
        }
    }

    pub fn with_burst_protection(mut self, burst_protection: bool) -> Self {
        self.burst_protection = Some(burst_protection);
        self
    }

    pub fn with_action(mut self, action: RateLimitAction) -> Self {
        self.action = Some(action);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interceptor_type_post_request_validation() {
        // Guardrails should be allowed in post-request
        let guardrail = InterceptorType::Guardrail {
            guard_id: "test".to_string(),
            config: GuardrailConfig {
                enabled: true,
                threshold: None,
                rules: None,
            },
        };
        assert!(guardrail.is_allowed_in_post_request());

        // Message transformers should not be allowed in post-request
        let transformer = InterceptorType::MessageTransformer {
            rules: vec![],
            direction: TransformDirection::PreRequest,
        };
        assert!(!transformer.is_allowed_in_post_request());

        // Rate limiters should not be allowed in post-request
        let rate_limiter = InterceptorType::RateLimiter {
            limit: 100,
            limit_target: LimitTarget::Requests,
            limit_entity: LimitEntity::UserName,
            period: RateLimitPeriod::Hour,
            burst_protection: None,
            action: None,
        };
        assert!(!rate_limiter.is_allowed_in_post_request());
    }

    #[test]
    fn test_rate_limiter_builder() {
        let rate_limiter = RateLimiter::new(
            1000,
            LimitTarget::Requests,
            LimitEntity::UserName,
            RateLimitPeriod::Hour,
        )
        .with_burst_protection(true)
        .with_action(RateLimitAction::Block);

        assert_eq!(rate_limiter.limit, 1000);
        assert_eq!(rate_limiter.burst_protection, Some(true));
        assert!(matches!(rate_limiter.action, Some(RateLimitAction::Block)));
    }

    #[test]
    fn test_interceptor_type_serialization() {
        let guardrail = InterceptorType::Guardrail {
            guard_id: "content_filter".to_string(),
            config: GuardrailConfig {
                enabled: true,
                threshold: Some(0.8),
                rules: Some(vec!["no_harmful_content".to_string()]),
            },
        };

        let json = serde_json::to_string(&guardrail).unwrap();
        assert!(json.contains("content_filter"));
        assert!(json.contains("0.8"));
    }
}