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
    Aws(AwsCredentials),
    // Hosted LangDB AWS
    // #[serde(other)]
    LangDb,
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
#[serde(deny_unknown_fields)]
pub struct AwsCredentials {
    pub access_key: String,
    pub access_secret: String,
    // Defaults tp us-east-1
    pub region: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::types::credentials::{ApiKeyCredentials, Credentials};

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
}
