use langdb_core::{
    models::ModelMetadata,
    pricing::calculator::{calculate_image_price, calculate_tokens_cost},
    types::{
        gateway::{CostCalculationResult, CostCalculator, CostCalculatorError, Usage},
        provider::ModelPrice,
    },
};

#[derive(Clone)]
pub struct GatewayCostCalculator {
    models: Vec<ModelMetadata>,
    default_image_cost: f64,
    default_input_cost: f64,
    default_output_cost: f64,
}

impl GatewayCostCalculator {
    pub fn new(models: Vec<ModelMetadata>) -> Self {
        Self {
            models,
            default_image_cost: 0.0,
            default_input_cost: 0.0,
            default_output_cost: 0.0,
        }
    }
}

#[async_trait::async_trait]
impl CostCalculator for GatewayCostCalculator {
    async fn calculate_cost(
        &self,
        model_name: &str,
        provider_name: &str,
        usage: &Usage,
    ) -> Result<CostCalculationResult, CostCalculatorError> {
        let model_name =
            if let Some(stripped) = model_name.strip_prefix(&format!("{}/", provider_name)) {
                stripped
            } else {
                model_name
            };

        let model = self.models.iter().find(|m| {
            (m.model.to_lowercase() == model_name.to_lowercase()
                || m.inference_provider.model_name.to_string().to_lowercase()
                    == model_name.to_lowercase())
                && m.inference_provider.provider.to_string() == *provider_name
        });

        if let Some(model) = model {
            let price = Some(model.price.clone());
            match usage {
                langdb_core::types::gateway::Usage::ImageGenerationModelUsage(usage) => {
                    if let Some(ModelPrice::ImageGeneration(p)) = &price {
                        Ok(calculate_image_price(p, usage, self.default_image_cost))
                    } else {
                        Err(CostCalculatorError::CalculationError(
                            "Image model pricing are not set".to_string(),
                        ))
                    }
                }
                langdb_core::types::gateway::Usage::CompletionModelUsage(usage) => {
                    let (input_price, output_price) = match price {
                        Some(p) => match p {
                            ModelPrice::Completion(c) => (c.per_input_token, c.per_output_token),
                            ModelPrice::Embedding(c) => (c.per_input_token, 0.0),
                            ModelPrice::ImageGeneration(_) => {
                                return Err(CostCalculatorError::CalculationError(
                                    "Model pricing not supported".to_string(),
                                ))
                            }
                        },
                        None => {
                            tracing::error!("Model not found: {model_name} - {provider_name}");
                            (self.default_input_cost, self.default_output_cost)
                        }
                    };
                    Ok(calculate_tokens_cost(usage, input_price, output_price))
                }
            }
        } else {
            tracing::error!("Model not found: {model_name} - {provider_name}");
            return Err(CostCalculatorError::ModelNotFound);
        }
    }
}
