use std::sync::Arc;

use langdb_core::{
    types::gateway::{CostCalculator, CostCalculatorError, Usage},
    usage::{InMemoryStorage, LimitPeriod},
};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::{cost::GatewayCostCalculator, limit::LLM_USAGE};

#[derive(Error, Debug)]
pub enum UsageSetError {
    #[error(transparent)]
    CostCalculatorError(#[from] CostCalculatorError),
}

pub(crate) async fn update_usage(
    storage: Arc<Mutex<InMemoryStorage>>,
    calculator: &GatewayCostCalculator,
    model_name: &str,
    provider_name: &str,
    model_usage: Option<&Usage>,
) -> Result<(), UsageSetError> {
    if let Some(usage) = model_usage {
        let cost = calculator
            .calculate_cost(model_name, provider_name, usage)
            .await?
            .cost;

        let v = storage
            .lock()
            .await
            .increment_and_get_value(LimitPeriod::Day, "default", LLM_USAGE, cost)
            .await;
        tracing::debug!(target:"gateway::usage", "Today usage: {v}");

        let v = storage
            .lock()
            .await
            .increment_and_get_value(LimitPeriod::Month, "default", LLM_USAGE, cost)
            .await;
        tracing::debug!(target:"gateway::usage", "Month usage: {v}");

        let v = storage
            .lock()
            .await
            .increment_and_get_value(LimitPeriod::Total, "default", LLM_USAGE, cost)
            .await;
        tracing::debug!(target:"gateway::usage", "Total usage: {v}");
    }

    Ok(())
}
