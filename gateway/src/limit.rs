use std::sync::Arc;

use langdb_core::{
    handler::{DollarUsage, LimitCheck},
    redis::aio::ConnectionManager,
    usage::{get_value, LimitPeriod},
};
use tokio::sync::Mutex;

use crate::config::CostControl;

pub const LLM_USAGE: &str = "llm_usage";
pub struct GatewayLimitChecker {
    client: Arc<Mutex<ConnectionManager>>,
    cost_control: CostControl,
}

impl GatewayLimitChecker {
    pub fn new(client: Arc<Mutex<ConnectionManager>>, cost_control: CostControl) -> Self {
        Self {
            client,
            cost_control,
        }
    }
}

impl GatewayLimitChecker {
    pub async fn _get_limits(
        &self,
        tenant_name: &str,
    ) -> Result<DollarUsage, Box<dyn std::error::Error>> {
        let mut client = self.client.lock().await;
        let total_usage: Option<f64> =
            get_value(&mut client, LimitPeriod::Total, tenant_name, LLM_USAGE).await?;
        let monthly_usage: Option<f64> =
            get_value(&mut client, LimitPeriod::Month, tenant_name, LLM_USAGE).await?;
        let daily_usage: Option<f64> =
            get_value(&mut client, LimitPeriod::Day, tenant_name, LLM_USAGE).await?;

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
    async fn can_execute_llm(
        &mut self,
        tenant_name: &str,
        project_id: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        self.get_usage(tenant_name, project_id).await.map(|usage| {
            usage.daily < usage.daily_limit.unwrap_or(f64::MAX)
                && usage.monthly < usage.monthly_limit.unwrap_or(f64::MAX)
                && usage.total < (usage.total_limit.unwrap_or(f64::MAX))
        })
    }
    async fn get_usage(
        &self,
        tenant_name: &str,
        _project_id: &str,
    ) -> Result<DollarUsage, Box<dyn std::error::Error>> {
        self._get_limits(tenant_name).await
    }
}
