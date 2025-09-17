use std::sync::Arc;

use langdb_core::{
    handler::{DollarUsage, LimitCheck},
    usage::{InMemoryStorage, LimitPeriod},
};
use tokio::sync::Mutex;

use crate::config::CostControl;

pub const LLM_USAGE: &str = "llm_usage";
pub struct GatewayLimitChecker {
    storage: Arc<Mutex<InMemoryStorage>>,
    cost_control: CostControl,
}

impl GatewayLimitChecker {
    pub fn new(storage: Arc<Mutex<InMemoryStorage>>, cost_control: CostControl) -> Self {
        Self {
            storage,
            cost_control,
        }
    }
}

impl GatewayLimitChecker {
    pub async fn _get_limits(&self) -> Result<DollarUsage, Box<dyn std::error::Error>> {
        let total_usage: Option<f64> =
            self.storage
                .lock()
                .await
                .get_value(&LimitPeriod::Total, "gateway", LLM_USAGE);
        let monthly_usage: Option<f64> =
            self.storage
                .lock()
                .await
                .get_value(&LimitPeriod::Month, "gateway", LLM_USAGE);
        let daily_usage: Option<f64> =
            self.storage
                .lock()
                .await
                .get_value(&LimitPeriod::Day, "gateway", LLM_USAGE);

        Ok(DollarUsage {
            daily: daily_usage.unwrap_or(0.0),
            daily_limit: self.cost_control.daily,
            monthly: monthly_usage.unwrap_or(0.0),
            monthly_limit: self.cost_control.monthly,
            total: total_usage.unwrap_or(0.0),
            total_limit: self.cost_control.total,
        })
    }
}

#[async_trait::async_trait]
impl LimitCheck for GatewayLimitChecker {
    #[tracing::instrument(level = "debug", skip(self))]
    async fn can_execute_llm(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        self.get_usage().await.map(|usage| {
            usage.daily < usage.daily_limit.unwrap_or(f64::MAX)
                && usage.monthly < usage.monthly_limit.unwrap_or(f64::MAX)
                && usage.total < (usage.total_limit.unwrap_or(f64::MAX))
        })
    }
    async fn get_usage(&self) -> Result<DollarUsage, Box<dyn std::error::Error>> {
        self._get_limits().await
    }
}
