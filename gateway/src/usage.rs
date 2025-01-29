use langdb_core::{
    redis::{self, aio::ConnectionManager},
    types::gateway::{CostCalculator, CostCalculatorError, Usage},
    usage::{increment_and_get_value, LimitPeriod},
};
use thiserror::Error;

use crate::{cost::GatewayCostCalculator, limit::LLM_USAGE};

#[derive(Error, Debug)]
pub enum UsageSetError {
    #[error(transparent)]
    CostCalculatorError(#[from] CostCalculatorError),

    #[error(transparent)]
    RedisError(#[from] redis::RedisError),
}

pub(crate) async fn update_usage(
    client: &mut ConnectionManager,
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

        let v =
            increment_and_get_value::<f64>(client, LimitPeriod::Day, "default", LLM_USAGE, cost)
                .await?;
        tracing::debug!(target:"gateway::usage", "Today usage: {v}");

        let v =
            increment_and_get_value::<f64>(client, LimitPeriod::Month, "default", LLM_USAGE, cost)
                .await?;
        tracing::debug!(target:"gateway::usage", "Month usage: {v}");

        let v =
            increment_and_get_value::<f64>(client, LimitPeriod::Total, "default", LLM_USAGE, cost)
                .await?;
        tracing::debug!(target:"gateway::usage", "Total usage: {v}");
    }

    Ok(())
}
