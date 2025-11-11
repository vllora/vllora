use serde::{Deserialize, Serialize};

/// Information about a provider's credential status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCredentialsInfo {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub has_credentials: bool,
}
