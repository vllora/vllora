use vllora_core::types::guardrails::partner::GuardPartner;
use vllora_core::types::guardrails::{evaluator::Evaluator, Guard, GuardResult};

use vllora_core::{model::ModelInstance, types::gateway::ChatCompletionMessage};

#[async_trait::async_trait]
pub trait GuardModelInstanceFactory: Send + Sync {
    async fn init(&self, name: &str) -> Box<dyn ModelInstance>;
}

pub struct PartnerEvaluator {
    partner_impl: Box<dyn GuardPartner + Send + Sync>,
}

impl PartnerEvaluator {
    pub fn new(partner_impl: Box<dyn GuardPartner + Send + Sync>) -> Self {
        Self { partner_impl }
    }
}

#[async_trait::async_trait]
impl Evaluator for PartnerEvaluator {
    async fn evaluate(
        &self,
        messages: &[ChatCompletionMessage],
        guard: &Guard,
    ) -> Result<GuardResult, String> {
        if let Guard::Partner { .. } = &guard {
            self.partner_impl
                .evaluate(messages)
                .await
                .map_err(|e| e.to_string())
        } else {
            Err("Guard definition is not a Partner".to_string())
        }
    }
}
