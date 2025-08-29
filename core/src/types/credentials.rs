use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum Credentials {
    ApiKey(ApiKeyCredentials),
    ApiKeyWithEndpoint {
        #[serde(alias = "ApiKey")]
        api_key: String,
        endpoint: String,
    },
    Aws(BedrockCredentials),
    Vertex(VertexCredentials),
    // Hosted LangDB AWS
    // #[serde(other)]
    LangDb,
}

impl Credentials {
    pub fn to_bedrock_credentials(&self) -> Option<BedrockCredentials> {
        match self {
            Credentials::Aws(bedrock) => Some(bedrock.clone()),
            Credentials::ApiKey(key) => Some(BedrockCredentials::ApiKey(AwsApiKeyCredentials {
                api_key: key.api_key.clone(),
                region: None,
            })),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct IntegrationCredentials {
    pub secrets: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApiKeyCredentials {
    #[serde(alias = "ApiKey")]
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct AwsIAMCredentials {
    pub access_key: String,
    pub access_secret: String,
    // Defaults tp us-east-1
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct AwsApiKeyCredentials {
    pub api_key: String,
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub enum BedrockCredentials {
    IAM(AwsIAMCredentials),
    ApiKey(AwsApiKeyCredentials),
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct VertexCredentials {
    pub region: String,
    pub r#type: String,
    pub project_id: String,
    pub private_key_id: String,
    pub private_key: String,
}

#[cfg(test)]
mod tests {
    use crate::types::credentials::AwsIAMCredentials;
    use crate::types::credentials::{
        ApiKeyCredentials, AwsApiKeyCredentials, BedrockCredentials, Credentials,
    };

    #[test]
    fn test_serialization() {
        let credentials = Credentials::ApiKey(ApiKeyCredentials {
            api_key: "api_key".to_string(),
        });
        let serialized = serde_json::to_string(&credentials).unwrap();
        let deserialized: Credentials = serde_json::from_str(&serialized).unwrap();
        assert_eq!(credentials, deserialized);

        let credentials = Credentials::ApiKeyWithEndpoint {
            api_key: "api_key".to_string(),
            endpoint: "https://my_own_endpoint.com".to_string(),
        };
        let serialized = serde_json::to_string(&credentials).unwrap();
        let deserialized: Credentials = serde_json::from_str(&serialized).unwrap();
        assert_eq!(credentials, deserialized);
    }

    #[test]
    fn test_bedrock_credentials() {
        let credentials = BedrockCredentials::ApiKey(AwsApiKeyCredentials {
            api_key: "api_key".to_string(),
            region: Some("us-east-1".to_string()),
        });
        let serialized = serde_json::to_string(&credentials).unwrap();
        let deserialized: BedrockCredentials = serde_json::from_str(&serialized).unwrap();
        assert_eq!(credentials, deserialized);
    }

    #[test]
    fn test_bedrock_credentials_iam() {
        let credentials = BedrockCredentials::IAM(AwsIAMCredentials {
            access_key: "access_key".to_string(),
            access_secret: "access_secret".to_string(),
            region: Some("us-east-1".to_string()),
        });

        let serialized = serde_json::to_string(&credentials).unwrap();
        let deserialized: BedrockCredentials = serde_json::from_str(&serialized).unwrap();
        assert_eq!(credentials, deserialized);
    }

    #[test]
    fn test_backwards_compatibility() {
        let credentials = AwsIAMCredentials {
            access_key: "access_key".to_string(),
            access_secret: "access_secret".to_string(),
            region: Some("us-east-1".to_string()),
        };
        let serialized = serde_json::to_string(&credentials).unwrap();
        let deserialized: AwsIAMCredentials = serde_json::from_str(&serialized).unwrap();
        assert_eq!(credentials, deserialized);
    }
}
