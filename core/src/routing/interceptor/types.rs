use serde::{Deserialize, Serialize};

/// Represents different types of interceptors that can be configured
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterceptorType {
    Guardrail {
        guard_id: String,
    },
    MessageTransformer {
        rules: Vec<TransformRule>,
        direction: TransformDirection, // pre_request, post_response
    },
    MetadataEnricher {
        fields: Vec<String>,
        sources: Vec<MetadataSource>,
    },
}

impl InterceptorType {
    /// Check if this interceptor type is allowed in post-request interceptors
    pub fn is_allowed_in_post_request(&self) -> bool {
        matches!(self, InterceptorType::Guardrail { .. })
    }

    /// Get the name/identifier for this interceptor type
    pub fn get_name(&self) -> &str {
        match self {
            InterceptorType::Guardrail { guard_id, .. } => guard_id,
            InterceptorType::MessageTransformer { .. } => "message_transformer",
            InterceptorType::MetadataEnricher { .. } => "metadata_enricher",
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interceptor_type_post_request_validation() {
        // Guardrails should be allowed in post-request
        let guardrail = InterceptorType::Guardrail {
            guard_id: "test".to_string(),
        };
        assert!(guardrail.is_allowed_in_post_request());

        // Message transformers should not be allowed in post-request
        let transformer = InterceptorType::MessageTransformer {
            rules: vec![],
            direction: TransformDirection::PreRequest,
        };
        assert!(!transformer.is_allowed_in_post_request());
    }

    #[test]
    fn test_interceptor_type_serialization() {
        let guardrail = InterceptorType::Guardrail {
            guard_id: "content_filter".to_string(),
        };

        let json = serde_json::to_string(&guardrail).unwrap();
        assert!(json.contains("content_filter"));
    }
}
