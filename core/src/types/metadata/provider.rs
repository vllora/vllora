/// Information about a provider with credential status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub endpoint: Option<String>,
    pub priority: i32,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub provider_type: String,
    pub has_credentials: bool,
}
