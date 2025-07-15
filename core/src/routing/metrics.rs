use std::collections::BTreeMap;

use crate::{routing::RouterError, usage::ProviderMetrics};

/// Trait for accessing metrics data needed for routing decisions
#[async_trait::async_trait]
pub trait MetricsRepository {
    /// Fetch metrics for all providers and models
    async fn get_metrics(&self) -> Result<BTreeMap<String, ProviderMetrics>, RouterError>;

    /// Fetch metrics for a specific provider
    async fn get_provider_metrics(
        &self,
        provider: &str,
    ) -> Result<Option<ProviderMetrics>, RouterError>;

    /// Fetch metrics for a specific model from a specific provider
    async fn get_model_metrics(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<Option<crate::usage::ModelMetrics>, RouterError>;
}

/// Simple in-memory implementation of MetricsRepository
/// This can be used as a reference implementation or for testing
pub struct InMemoryMetricsRepository {
    metrics: BTreeMap<String, ProviderMetrics>,
}

impl InMemoryMetricsRepository {
    pub fn new(metrics: BTreeMap<String, ProviderMetrics>) -> Self {
        Self { metrics }
    }
}

#[async_trait::async_trait]
impl MetricsRepository for InMemoryMetricsRepository {
    async fn get_metrics(&self) -> Result<BTreeMap<String, ProviderMetrics>, RouterError> {
        Ok(self.metrics.clone())
    }

    async fn get_provider_metrics(
        &self,
        provider: &str,
    ) -> Result<Option<ProviderMetrics>, RouterError> {
        Ok(self.metrics.get(provider).cloned())
    }

    async fn get_model_metrics(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<Option<crate::usage::ModelMetrics>, RouterError> {
        Ok(self
            .metrics
            .get(provider)
            .and_then(|provider_metrics| provider_metrics.models.get(model))
            .cloned())
    }
}
