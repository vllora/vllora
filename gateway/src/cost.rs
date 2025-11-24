use vllora_core::pricing::calculator::{calculate_image_price, calculate_tokens_cost};
use vllora_llm::types::credentials_ident::CredentialsIdent;
use vllora_llm::types::gateway::{
    CostCalculationResult, CostCalculator, CostCalculatorError, Usage,
};
use vllora_llm::types::provider::ModelPrice;

#[derive(Clone)]
pub struct GatewayCostCalculator {
    default_image_cost: f64,
}

impl GatewayCostCalculator {
    pub fn new() -> Self {
        Self {
            default_image_cost: 0.0,
        }
    }
}

#[async_trait::async_trait]
impl CostCalculator for GatewayCostCalculator {
    async fn calculate_cost(
        &self,
        price: &ModelPrice,
        usage: &Usage,
        _credentials_ident: &CredentialsIdent,
    ) -> Result<CostCalculationResult, CostCalculatorError> {
        match usage {
            vllora_llm::types::gateway::Usage::ImageGenerationModelUsage(usage) => {
                if let ModelPrice::ImageGeneration(p) = &price {
                    Ok(calculate_image_price(p, usage, self.default_image_cost))
                } else {
                    Err(CostCalculatorError::CalculationError(
                        "Image model pricing are not set".to_string(),
                    ))
                }
            }
            vllora_llm::types::gateway::Usage::CompletionModelUsage(usage) => {
                let (input_price, cached_input_price, cached_input_write_price, output_price) =
                    match price {
                        ModelPrice::Completion(c) => (
                            c.per_input_token,
                            c.per_cached_input_token,
                            c.per_cached_input_write_token,
                            c.per_output_token,
                        ),
                        ModelPrice::Embedding(c) => (c.per_input_token, None, None, 0.0),
                        ModelPrice::ImageGeneration(_) => {
                            return Err(CostCalculatorError::CalculationError(
                                "Model pricing not supported".to_string(),
                            ))
                        }
                    };
                Ok(calculate_tokens_cost(
                    usage,
                    input_price,
                    cached_input_price,
                    cached_input_write_price,
                    output_price,
                ))
            }
        }
    }
}
