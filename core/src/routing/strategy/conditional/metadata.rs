use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use vllora_llm::types::gateway::Extra;

#[derive(Error, Debug)]
pub enum MetadataError {
    #[error("Metadata extraction failed: {0}")]
    ExtractionError(String),

    #[error("Invalid metadata field: {0}")]
    InvalidFieldError(String),

    #[error("Metadata validation failed: {0}")]
    ValidationError(String),
}

/// Represents different types of metadata fields that can be extracted
#[derive(Debug, Clone, PartialEq)]
pub enum MetadataField {
    // User metadata from Extra.user
    UserId,
    UserName,
    UserEmail,
    UserTiers,
    UserTier,

    // Dynamic variables from Extra.variables
    Variable(String), // Dynamic access to Extra.variables

    // Guardrail results from Extra.guards
    GuardrailResult(String), // Access guardrail results
}

impl MetadataField {
    /// Extract the value for this metadata field from the Extra struct
    pub fn extract(&self, extra: Option<&Extra>) -> Result<Option<Value>, MetadataError> {
        match self {
            MetadataField::UserId => {
                if let Some(extra) = extra {
                    if let Some(user) = &extra.user {
                        Ok(user.id.as_ref().map(|id| Value::String(id.clone())))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }

            MetadataField::UserName => {
                if let Some(extra) = extra {
                    if let Some(user) = &extra.user {
                        Ok(user.name.as_ref().map(|name| Value::String(name.clone())))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }

            MetadataField::UserEmail => {
                if let Some(extra) = extra {
                    if let Some(user) = &extra.user {
                        Ok(user
                            .email
                            .as_ref()
                            .map(|email| Value::String(email.clone())))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }

            MetadataField::UserTier => {
                if let Some(extra) = extra {
                    if let Some(user) = &extra.user {
                        if let Some(tiers) = &user.tiers {
                            Ok(tiers.first().map(|tier| Value::String(tier.clone())))
                        } else {
                            Ok(None)
                        }
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }

            MetadataField::UserTiers => {
                if let Some(extra) = extra {
                    if let Some(user) = &extra.user {
                        if let Some(tiers) = &user.tiers {
                            let tiers_array: Vec<Value> = tiers
                                .iter()
                                .map(|tier| Value::String(tier.clone()))
                                .collect();
                            Ok(Some(Value::Array(tiers_array)))
                        } else {
                            Ok(None)
                        }
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }

            MetadataField::Variable(var_name) => {
                if let Some(extra) = extra {
                    if let Some(variables) = &extra.variables {
                        Ok(variables.get(var_name).cloned())
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }

            MetadataField::GuardrailResult(_guard_id) => {
                if let Some(_extra) = extra {
                    // For now, we'll return a placeholder since guardrail results
                    // need to be integrated with the actual guardrail system
                    // This will be enhanced when we implement the guardrail system
                    Ok(Some(Value::Bool(true))) // Placeholder
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Parse a metadata field from a string representation
    pub fn from_string(field_str: &str) -> Result<Self, MetadataError> {
        match field_str {
            "user.id" => Ok(MetadataField::UserId),
            "user.name" => Ok(MetadataField::UserName),
            "user.email" => Ok(MetadataField::UserEmail),
            "user.tiers" => Ok(MetadataField::UserTiers),
            "user.tier" => Ok(MetadataField::UserTier),
            s if s.starts_with("variables.") => {
                let var_name = s.strip_prefix("variables.").unwrap();
                Ok(MetadataField::Variable(var_name.to_string()))
            }
            s if s.starts_with("guards.") => {
                let guard_id = s.strip_prefix("guards.").unwrap();
                Ok(MetadataField::GuardrailResult(guard_id.to_string()))
            }
            _ => Err(MetadataError::InvalidFieldError(field_str.to_string())),
        }
    }
}

impl std::fmt::Display for MetadataField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetadataField::UserId => write!(f, "user.id"),
            MetadataField::UserName => write!(f, "user.name"),
            MetadataField::UserEmail => write!(f, "user.email"),
            MetadataField::UserTiers => write!(f, "user.tiers"),
            MetadataField::UserTier => write!(f, "user.tier"),
            MetadataField::Variable(var_name) => write!(f, "variables.{var_name}"),
            MetadataField::GuardrailResult(guard_id) => write!(f, "guards.{guard_id}"),
        }
    }
}

/// Manages metadata extraction and caching
pub struct MetadataManager {
    cache: HashMap<String, (Value, std::time::Instant)>,
    cache_ttl: std::time::Duration,
}

impl MetadataManager {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            cache_ttl: std::time::Duration::from_secs(300), // 5 minutes default TTL
        }
    }

    pub fn with_cache_ttl(mut self, ttl: std::time::Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Extract metadata for a specific field
    pub fn extract_metadata(
        &mut self,
        field: &MetadataField,
        extra: Option<&Extra>,
    ) -> Result<Option<Value>, MetadataError> {
        let cache_key = field.to_string();

        // Check cache first
        if let Some((cached_value, timestamp)) = self.cache.get(&cache_key) {
            if timestamp.elapsed() < self.cache_ttl {
                return Ok(Some(cached_value.clone()));
            } else {
                // Cache expired, remove it
                self.cache.remove(&cache_key);
            }
        }

        // Extract fresh value
        let result = field.extract(extra)?;

        // Cache the result if it exists
        if let Some(ref value) = result {
            self.cache
                .insert(cache_key, (value.clone(), std::time::Instant::now()));
        }

        Ok(result)
    }

    /// Extract all metadata from Extra fields
    pub fn extract_all_metadata(
        &mut self,
        extra: Option<&Extra>,
    ) -> Result<HashMap<String, Value>, MetadataError> {
        let mut metadata = HashMap::new();

        // Extract user metadata
        if let Some(extra) = extra {
            if let Some(user) = &extra.user {
                if let Some(id) = &user.id {
                    metadata.insert("user.id".to_string(), Value::String(id.clone()));
                }
                if let Some(name) = &user.name {
                    metadata.insert("user.name".to_string(), Value::String(name.clone()));
                }
                if let Some(email) = &user.email {
                    metadata.insert("user.email".to_string(), Value::String(email.clone()));
                }
                if let Some(tiers) = &user.tiers {
                    let tiers_array: Vec<Value> = tiers
                        .iter()
                        .map(|tier| Value::String(tier.clone()))
                        .collect();
                    metadata.insert("user.tiers".to_string(), Value::Array(tiers_array));
                }
            }

            // Extract variables
            if let Some(variables) = &extra.variables {
                for (key, value) in variables {
                    metadata.insert(format!("variables.{key}"), value.clone());
                }
            }

            // Extract guardrail results (placeholder for now)
            for guard in &extra.guards {
                match guard {
                    vllora_llm::types::gateway::GuardOrName::GuardId(guard_id) => {
                        metadata.insert(format!("guards.{guard_id}"), Value::Bool(true));
                    }
                    vllora_llm::types::gateway::GuardOrName::GuardWithParameters(
                        guard_with_params,
                    ) => {
                        metadata.insert(
                            format!("guards.{}", guard_with_params.id),
                            Value::Bool(true),
                        );
                    }
                }
            }
        }

        Ok(metadata)
    }

    /// Clear the metadata cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, std::time::Duration) {
        (self.cache.len(), self.cache_ttl)
    }
}

impl Default for MetadataManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vllora_llm::types::gateway::{Extra, RequestUser};

    #[test]
    fn test_metadata_field_parsing() {
        assert_eq!(
            MetadataField::from_string("user.id").unwrap(),
            MetadataField::UserId
        );
        assert_eq!(
            MetadataField::from_string("user.name").unwrap(),
            MetadataField::UserName
        );
        assert_eq!(
            MetadataField::from_string("user.email").unwrap(),
            MetadataField::UserEmail
        );
        assert_eq!(
            MetadataField::from_string("user.tiers").unwrap(),
            MetadataField::UserTiers
        );
        assert_eq!(
            MetadataField::from_string("variables.test_var").unwrap(),
            MetadataField::Variable("test_var".to_string())
        );
        assert_eq!(
            MetadataField::from_string("guards.toxicity").unwrap(),
            MetadataField::GuardrailResult("toxicity".to_string())
        );

        assert!(MetadataField::from_string("invalid.field").is_err());
    }

    #[test]
    fn test_metadata_field_to_string() {
        assert_eq!(MetadataField::UserId.to_string(), "user.id");
        assert_eq!(MetadataField::UserName.to_string(), "user.name");
        assert_eq!(MetadataField::UserEmail.to_string(), "user.email");
        assert_eq!(MetadataField::UserTiers.to_string(), "user.tiers");
        assert_eq!(
            MetadataField::Variable("test_var".to_string()).to_string(),
            "variables.test_var"
        );
        assert_eq!(
            MetadataField::GuardrailResult("toxicity".to_string()).to_string(),
            "guards.toxicity"
        );
    }

    #[test]
    fn test_user_metadata_extraction() {
        let user = RequestUser {
            id: Some("user123".to_string()),
            name: Some("John Doe".to_string()),
            email: Some("john@example.com".to_string()),
            tiers: Some(vec!["premium".to_string(), "enterprise".to_string()]),
        };

        let extra = Some(Extra {
            user: Some(user),
            guards: vec![],
            cache: None,
            variables: None,
        });

        assert_eq!(
            MetadataField::UserId.extract(extra.as_ref()).unwrap(),
            Some(Value::String("user123".to_string()))
        );

        assert_eq!(
            MetadataField::UserName.extract(extra.as_ref()).unwrap(),
            Some(Value::String("John Doe".to_string()))
        );

        assert_eq!(
            MetadataField::UserEmail.extract(extra.as_ref()).unwrap(),
            Some(Value::String("john@example.com".to_string()))
        );

        let tiers = MetadataField::UserTiers
            .extract(extra.as_ref())
            .unwrap()
            .unwrap();
        assert_eq!(tiers.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_variables_metadata_extraction() {
        let mut variables = HashMap::new();
        variables.insert("priority".to_string(), Value::String("high".to_string()));
        variables.insert(
            "department".to_string(),
            Value::String("engineering".to_string()),
        );
        variables.insert(
            "budget".to_string(),
            Value::Number(serde_json::Number::from(1000)),
        );

        let extra = Some(Extra {
            user: None,
            guards: vec![],
            cache: None,
            variables: Some(variables),
        });

        assert_eq!(
            MetadataField::Variable("priority".to_string())
                .extract(extra.as_ref())
                .unwrap(),
            Some(Value::String("high".to_string()))
        );

        assert_eq!(
            MetadataField::Variable("department".to_string())
                .extract(extra.as_ref())
                .unwrap(),
            Some(Value::String("engineering".to_string()))
        );

        assert_eq!(
            MetadataField::Variable("budget".to_string())
                .extract(extra.as_ref())
                .unwrap(),
            Some(Value::Number(serde_json::Number::from(1000)))
        );

        assert_eq!(
            MetadataField::Variable("nonexistent".to_string())
                .extract(extra.as_ref())
                .unwrap(),
            None
        );
    }

    #[test]
    fn test_metadata_manager() {
        let mut manager = MetadataManager::new();

        let user = RequestUser {
            id: Some("user123".to_string()),
            name: Some("John Doe".to_string()),
            email: None,
            tiers: Some(vec!["premium".to_string()]),
        };

        let extra = Some(Extra {
            user: Some(user),
            guards: vec![],
            cache: None,
            variables: None,
        });

        let metadata = manager.extract_all_metadata(extra.as_ref()).unwrap();

        assert_eq!(
            metadata.get("user.id").unwrap(),
            &Value::String("user123".to_string())
        );
        assert_eq!(
            metadata.get("user.name").unwrap(),
            &Value::String("John Doe".to_string())
        );
        assert_eq!(metadata.get("user.email"), None);

        let tiers = metadata.get("user.tiers").unwrap().as_array().unwrap();
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0], Value::String("premium".to_string()));
    }
}
