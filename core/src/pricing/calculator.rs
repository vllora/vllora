use crate::types::{
    gateway::{
        CompletionModelUsage, CostCalculationResult, ImageCostCalculationResult,
        ImageGenerationModelUsage,
    },
    provider::ImageGenerationPrice,
};

pub fn calculate_image_price(
    p: &ImageGenerationPrice,
    usage: &ImageGenerationModelUsage,
    default_image_cost: f64,
) -> CostCalculationResult {
    if let Some(type_prices) = &p.type_prices {
        let size = format!("{}x{}", usage.size.0, usage.size.1);
        let type_price = match type_prices.get(&usage.quality) {
            Some(resolution_prices) => resolution_prices
                .get(&size)
                .map_or(default_image_cost, |p| *p),
            None => default_image_cost,
        };

        CostCalculationResult {
            cost: (usage.images_count * usage.steps_count) as f64 * type_price,
            per_input_token: 0.0,
            per_output_token: 0.0,
            per_image_cost: Some(ImageCostCalculationResult::TypePrice {
                size: size.clone(),
                quality: usage.quality.clone(),
                per_image: type_price,
            }),
        }
    } else if let Some(cost) = p.mp_price {
        let total_mp = (usage.size.0 as f64 * usage.size.1 as f64 * usage.images_count as f64)
            / 1024.0
            / 1024.0;
        CostCalculationResult {
            cost: cost * total_mp * (usage.steps_count * usage.images_count) as f64,
            per_input_token: 0.0,
            per_output_token: 0.0,
            per_image_cost: Some(ImageCostCalculationResult::MPPrice(cost)),
        }
    } else {
        tracing::warn!("Image model pricing are not set");
        let price = default_image_cost;
        CostCalculationResult {
            cost: price * (usage.steps_count * usage.images_count) as f64,
            per_input_token: 0.0,
            per_output_token: 0.0,
            per_image_cost: Some(ImageCostCalculationResult::SingleImagePrice(price)),
        }
    }
}

pub fn calculate_tokens_cost(
    usage: &CompletionModelUsage,
    cost_per_input_token: f64,
    cost_per_output_token: f64,
) -> CostCalculationResult {
    let input_cost = cost_per_input_token * usage.input_tokens as f64 * 1e-6;
    let output_cost = cost_per_output_token * usage.output_tokens as f64 * 1e-6;

    CostCalculationResult {
        cost: input_cost + output_cost,
        per_input_token: cost_per_input_token,
        per_output_token: cost_per_output_token,
        per_image_cost: None,
    }
}
