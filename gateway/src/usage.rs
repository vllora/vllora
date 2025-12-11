use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::cost::GatewayCostCalculator;
use vllora_core::usage::{InMemoryStorage, LimitPeriod};
use vllora_llm::types::credentials_ident::CredentialsIdent;
use vllora_llm::types::gateway::CostCalculator;
use vllora_llm::types::gateway::CostCalculatorError;
use vllora_llm::types::gateway::GatewayModelUsage;
use vllora_llm::types::gateway::Usage;
use vllora_llm::types::provider::ModelPrice;

#[derive(Error, Debug)]
pub enum UsageSetError {
    #[error(transparent)]
    CostCalculatorError(#[from] CostCalculatorError),
}

pub const LLM_USAGE: &str = "llm_usage";
pub const INPUT_TOKENS: &str = "input_tokens";
pub const OUTPUT_TOKENS: &str = "output_tokens";
pub const TOTAL_TOKENS: &str = "total_tokens";
pub const REQUESTS: &str = "requests";
pub const REQUESTS_DURATION: &str = "requests_duration";
pub const TTFT: &str = "ttft";

#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_usage(
    storage: Arc<Mutex<InMemoryStorage>>,
    calculator: &GatewayCostCalculator,
    model_name: &str,
    provider_name: &str,
    model_usage: Option<&Usage>,
    duration: Option<u64>,
    ttft: Option<u64>,
    price: &ModelPrice,
) -> Result<(), UsageSetError> {
    if let Some(usage) = model_usage {
        let cost = calculator
            .calculate_cost(price, usage, &CredentialsIdent::Own)
            .await?
            .cost;

        let periods = [
            LimitPeriod::Hour,
            LimitPeriod::Day,
            LimitPeriod::Month,
            LimitPeriod::Total,
        ];
        for p in &periods {
            let v = storage
                .lock()
                .await
                .increment_and_get_value(p, "default", LLM_USAGE, cost)
                .await;
            tracing::debug!(target:"gateway::usage", "{p} usage: {v}");
        }

        match usage {
            Usage::CompletionModelUsage(GatewayModelUsage {
                input_tokens,
                output_tokens,
                total_tokens,
                ..
            }) => {
                let identifier = format!("{provider_name}:{model_name}");
                let mut values_tuples = vec![
                    (INPUT_TOKENS, *input_tokens as f64, "input tokens"),
                    (OUTPUT_TOKENS, *output_tokens as f64, "output tokens"),
                    (TOTAL_TOKENS, *total_tokens as f64, "total tokens"),
                    (REQUESTS, 1.0, "requests"),
                    (LLM_USAGE, cost, "cost"),
                ];

                if let Some(duration) = duration {
                    values_tuples.push((REQUESTS_DURATION, duration as f64, "duration"));
                }

                if let Some(ttft) = ttft {
                    values_tuples.push((TTFT, ttft as f64, "ttft"));
                }

                for p in &periods {
                    for (key, value, description) in &values_tuples {
                        let v = storage
                            .lock()
                            .await
                            .increment_and_get_value(p, &identifier, key, *value)
                            .await;
                        tracing::debug!(target:"gateway::usage", "{p} {description}: {v}");
                    }
                }

                let metrics = storage.lock().await.get_all_counters().await;

                tracing::debug!(target:"gateway::usage", metrics = %serde_yaml::to_string(&metrics).unwrap());
            }
            Usage::ImageGenerationModelUsage(_) => {}
        }
    }

    Ok(())
}
