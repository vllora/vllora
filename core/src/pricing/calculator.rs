use vllora_llm::types::{
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
            per_cached_input_token: None,
            per_cached_input_write_token: None,
            is_cache_used: false,
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
            per_cached_input_token: None,
            per_cached_input_write_token: None,
            is_cache_used: false,
            per_image_cost: Some(ImageCostCalculationResult::MPPrice(cost)),
        }
    } else {
        tracing::warn!("Image model pricing are not set");
        let price = default_image_cost;
        CostCalculationResult {
            cost: price * (usage.steps_count * usage.images_count) as f64,
            per_input_token: 0.0,
            per_output_token: 0.0,
            per_cached_input_token: None,
            per_cached_input_write_token: None,
            is_cache_used: false,
            per_image_cost: Some(ImageCostCalculationResult::SingleImagePrice(price)),
        }
    }
}

pub fn calculate_tokens_cost(
    usage: &CompletionModelUsage,
    mut cost_per_input_token: f64,
    mut cost_per_cached_input_token: Option<f64>,
    mut cost_per_cached_input_write_token: Option<f64>,
    mut cost_per_output_token: f64,
) -> CostCalculationResult {
    if usage.is_cache_used {
        cost_per_input_token /= 100.0;
        cost_per_cached_input_token = cost_per_cached_input_token.map(|c| c / 100.0);
        cost_per_cached_input_write_token = cost_per_cached_input_write_token.map(|c| c / 100.0);
        cost_per_output_token /= 100.0;
    }

    let cached_tokens = usage
        .prompt_tokens_details
        .as_ref()
        .map_or(0, |p| p.cached_tokens());
    let cached_input_write_tokens = usage
        .prompt_tokens_details
        .as_ref()
        .map_or(0, |p| p.cache_creation_tokens());
    let not_cached_input_tokens = usage
        .input_tokens
        .saturating_sub(cached_tokens)
        .saturating_sub(cached_input_write_tokens);

    let cached_input_token_cost = cost_per_cached_input_token.unwrap_or(cost_per_input_token);
    let cached_input_write_token_cost =
        cost_per_cached_input_write_token.unwrap_or(cost_per_input_token);

    let input_cost = cost_per_input_token * not_cached_input_tokens as f64 * 1e-6;
    let cached_input_cost = cached_input_token_cost * cached_tokens as f64 * 1e-6;
    let cached_input_write_cost =
        cached_input_write_token_cost * cached_input_write_tokens as f64 * 1e-6;
    let output_cost = cost_per_output_token * usage.output_tokens as f64 * 1e-6;

    CostCalculationResult {
        cost: input_cost + cached_input_cost + cached_input_write_cost + output_cost,
        per_input_token: cost_per_input_token,
        per_cached_input_token: cost_per_cached_input_token,
        per_cached_input_write_token: cost_per_cached_input_write_token,
        per_output_token: cost_per_output_token,
        per_image_cost: None,
        is_cache_used: usage.is_cache_used,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::gateway::{CompletionModelUsage, PromptTokensDetails};

    #[test]
    fn test_calculate_tokens_cost_no_cache() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: None,
            completion_tokens_details: None,
            is_cache_used: false,
        };

        let cost_per_input_token = 1.0; // $0.001 per input token
        let cost_per_cached_input_token = Some(0.5); // $0.0005 per cached input token
        let cost_per_cached_input_write_token = Some(0.3); // $0.0003 per cached input write token
        let cost_per_output_token = 2.0; // $0.002 per output token

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // Expected calculations:
        // input_cost = 1000 * 1 * 1e-6 = 0.001
        // cached_input_cost = 0 (no cached tokens)
        // cached_input_write_cost = 0 (no cached write tokens)
        // output_cost = 500 * 2 * 1e-6 = 0.001
        // total_cost = 0.001 + 0.0 + 0.0 + 0.001 = 0.002

        assert!((result.cost - 0.002).abs() < 1e-10);
        assert_eq!(result.per_input_token, 1.0);
        assert_eq!(result.per_cached_input_token, Some(0.5));
        assert_eq!(result.per_cached_input_write_token, Some(0.3));
        assert_eq!(result.per_output_token, 2.0);
        assert!(!result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_with_cache_no_cached_tokens() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(0), Some(0), None)),
            completion_tokens_details: None,
            is_cache_used: true,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // When cache is used, prices are divided by 100
        // input_cost = 1000 * (1/100) * 1e-6 = 0.00001
        // cached_input_cost = 0 * (0.5/100) * 1e-6 = 0.0
        // cached_input_write_cost = 0 * (0.3/100) * 1e-6 = 0.0
        // output_cost = 500 * (2/100) * 1e-6 = 0.00001
        // total_cost = 0.00001 + 0.0 + 0.0 + 0.00001 = 0.00002

        assert!((result.cost - 0.00002).abs() < 1e-10);
        assert_eq!(result.per_input_token, 0.01); // 1 / 100
        assert_eq!(result.per_cached_input_token, Some(0.005)); // 0.5 / 100
        assert_eq!(result.per_cached_input_write_token, Some(0.003)); // 0.3 / 100
        assert_eq!(result.per_output_token, 0.02); // 2 / 100
        assert!(result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_with_cache_and_cached_tokens() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(300), Some(0), None)),
            completion_tokens_details: None,
            is_cache_used: true,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // When cache is used, prices are divided by 100
        // input_tokens = 1000 - 300 = 700 (cached tokens subtracted)
        // input_cost = 700 * (1/100) * 1e-6 = 0.000007
        // cached_input_cost = 300 * (0.5/100) * 1e-6 = 0.0000015
        // cached_input_write_cost = 0 * (0.3/100) * 1e-6 = 0.0
        // output_cost = 500 * (2/100) * 1e-6 = 0.00001
        // total_cost = 0.000007 + 0.0000015 + 0.0 + 0.00001 = 0.0000185

        assert!((result.cost - 0.0000185).abs() < 1e-10);
        assert_eq!(result.per_input_token, 0.01); // 1 / 100
        assert_eq!(result.per_cached_input_token, Some(0.005)); // 0.5 / 100
        assert_eq!(result.per_cached_input_write_token, Some(0.003)); // 0.3 / 100
        assert_eq!(result.per_output_token, 0.02); // 2 / 100
        assert!(result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_with_cache_and_cached_write_tokens() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(300), Some(100), None)),
            completion_tokens_details: None,
            is_cache_used: true,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // When cache is used, prices are divided by 100
        // input_tokens = 1000 - 300 - 100 = 600 (cached tokens and cached write tokens subtracted)
        // input_cost = 600 * (1/100) * 1e-6 = 0.000006
        // cached_input_cost = 300 * (0.5/100) * 1e-6 = 0.0000015
        // cached_input_write_cost = 100 * (0.3/100) * 1e-6 = 0.0000003
        // output_cost = 500 * (2/100) * 1e-6 = 0.00001
        // total_cost = 0.000006 + 0.0000015 + 0.0000003 + 0.00001 = 0.0000178

        assert!((result.cost - 0.0000178).abs() < 1e-10);
        assert_eq!(result.per_input_token, 0.01); // 1 / 100
        assert_eq!(result.per_cached_input_token, Some(0.005)); // 0.5 / 100
        assert_eq!(result.per_cached_input_write_token, Some(0.003)); // 0.3 / 100
        assert_eq!(result.per_output_token, 0.02); // 2 / 100
        assert!(result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_with_cache_no_cached_input_token_price() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(300), Some(0), None)),
            completion_tokens_details: None,
            is_cache_used: true,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = None; // No specific cached input token price
        let cost_per_cached_input_write_token = None; // No specific cached input write token price
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // When cache is used, prices are divided by 100
        // Since no cached input token price is set, input_tokens = 700
        // input_cost = 700 * (1/100) * 1e-6 = 0.000007
        // cached_input_cost = 300 * (1/100) * 1e-6 = 0.000003 (uses input token price)
        // cached_input_write_cost = 0 * (1/100) * 1e-6 = 0.0 (uses input token price)
        // output_cost = 500 * (2/100) * 1e-6 = 0.00001
        // total_cost = 0.000007 + 0.000003 + 0.0 + 0.00001 = 0.00002

        assert!((result.cost - 0.00002).abs() < 1e-10);
        assert_eq!(result.per_input_token, 0.01); // 1 / 100
        assert_eq!(result.per_cached_input_token, None);
        assert_eq!(result.per_cached_input_write_token, None);
        assert_eq!(result.per_output_token, 0.02); // 2 / 100
        assert!(result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_no_cache_with_cached_tokens() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(300), Some(0), None)),
            completion_tokens_details: None,
            is_cache_used: false,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // No cache used, so prices are not divided
        // input_tokens = 1000 - 300 = 700 (cached tokens subtracted)
        // input_cost = 700 * 1 * 1e-6 = 0.0007
        // cached_input_cost = 300 * 0.5 * 1e-6 = 0.00015
        // cached_input_write_cost = 0 * 0.3 * 1e-6 = 0.0
        // output_cost = 500 * 2 * 1e-6 = 0.001
        // total_cost = 0.0007 + 0.00015 + 0.0 + 0.001 = 0.00185

        assert!((result.cost - 0.00185).abs() < 1e-10);
        assert_eq!(result.per_input_token, 1.0);
        assert_eq!(result.per_cached_input_token, Some(0.5));
        assert_eq!(result.per_cached_input_write_token, Some(0.3));
        assert_eq!(result.per_output_token, 2.0);
        assert!(!result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_no_cache_with_cached_write_tokens() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(300), Some(100), None)),
            completion_tokens_details: None,
            is_cache_used: false,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // No cache used, so prices are not divided
        // input_tokens = 1000 - 300 - 100 = 600 (cached tokens and cached write tokens subtracted)
        // input_cost = 600 * 1 * 1e-6 = 0.0006
        // cached_input_cost = 300 * 0.5 * 1e-6 = 0.00015
        // cached_input_write_cost = 100 * 0.3 * 1e-6 = 0.00003
        // output_cost = 500 * 2 * 1e-6 = 0.001
        // total_cost = 0.0006 + 0.00015 + 0.00003 + 0.001 = 0.00178

        assert!((result.cost - 0.00178).abs() < 1e-10);
        assert_eq!(result.per_input_token, 1.0);
        assert_eq!(result.per_cached_input_token, Some(0.5));
        assert_eq!(result.per_cached_input_write_token, Some(0.3));
        assert_eq!(result.per_output_token, 2.0);
        assert!(!result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_zero_tokens() {
        let usage = CompletionModelUsage {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            prompt_tokens_details: None,
            completion_tokens_details: None,
            is_cache_used: false,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // All costs should be zero
        assert!((result.cost - 0.0).abs() < 1e-10);
        assert_eq!(result.per_input_token, 1.0);
        assert_eq!(result.per_cached_input_token, Some(0.5));
        assert_eq!(result.per_cached_input_write_token, Some(0.3));
        assert_eq!(result.per_output_token, 2.0);
        assert!(!result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_large_numbers() {
        let usage = CompletionModelUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            total_tokens: 1_500_000,
            prompt_tokens_details: Some(PromptTokensDetails::new(
                Some(200_000),
                Some(50_000),
                None,
            )),
            completion_tokens_details: None,
            is_cache_used: false,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // input_tokens = 1_000_000 - 200_000 - 50_000 = 750_000
        // input_cost = 750_000 * 1 * 1e-6 = 0.75
        // cached_input_cost = 200_000 * 0.5 * 1e-6 = 0.1
        // cached_input_write_cost = 50_000 * 0.3 * 1e-6 = 0.015
        // output_cost = 500_000 * 2 * 1e-6 = 1.0
        // total_cost = 0.75 + 0.1 + 0.015 + 1.0 = 1.865

        assert!((result.cost - 1.865).abs() < 1e-10);
        assert_eq!(result.per_input_token, 1.0);
        assert_eq!(result.per_cached_input_token, Some(0.5));
        assert_eq!(result.per_cached_input_write_token, Some(0.3));
        assert_eq!(result.per_output_token, 2.0);
        assert!(!result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_cache_with_audio_tokens() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(300), Some(100), None)),
            completion_tokens_details: None,
            is_cache_used: true,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // Audio tokens are included in cached_tokens() method
        // cached_tokens = 300 (only cached_tokens, not audio_tokens)
        // cached_input_write_tokens = 100 (cache_creation_tokens)
        // input_tokens = 1000 - 300 - 100 = 600
        // input_cost = 600 * (1/100) * 1e-6 = 0.000006
        // cached_input_cost = 300 * (0.5/100) * 1e-6 = 0.0000015
        // cached_input_write_cost = 100 * (0.3/100) * 1e-6 = 0.0000003
        // output_cost = 500 * (2/100) * 1e-6 = 0.00001
        // total_cost = 0.000006 + 0.0000015 + 0.0000003 + 0.00001 = 0.0000178

        assert_eq!(result.cost, 0.0000178);
        assert_eq!(result.per_input_token, 0.01);
        assert_eq!(result.per_cached_input_token, Some(0.005));
        assert_eq!(result.per_cached_input_write_token, Some(0.003));
        assert_eq!(result.per_output_token, 0.02);
        assert!(result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_edge_case_all_cached_tokens() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(1000), Some(0), None)),
            completion_tokens_details: None,
            is_cache_used: false,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // All input tokens are cached
        // input_tokens = 1000 - 1000 = 0
        // input_cost = 0 * 1 * 1e-6 = 0.0
        // cached_input_cost = 1000 * 0.5 * 1e-6 = 0.0005
        // cached_input_write_cost = 0 * 0.3 * 1e-6 = 0.0
        // output_cost = 500 * 2 * 1e-6 = 0.001
        // total_cost = 0.0 + 0.0005 + 0.0 + 0.001 = 0.0015

        assert!((result.cost - 0.0015).abs() < 1e-10);
        assert_eq!(result.per_input_token, 1.0);
        assert_eq!(result.per_cached_input_token, Some(0.5));
        assert_eq!(result.per_cached_input_write_token, Some(0.3));
        assert_eq!(result.per_output_token, 2.0);
        assert!(!result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_edge_case_more_cached_than_input() {
        let usage = CompletionModelUsage {
            input_tokens: 500,
            output_tokens: 200,
            total_tokens: 700,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(1000), Some(0), None)),
            completion_tokens_details: None,
            is_cache_used: false,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // More cached tokens than input tokens (edge case)
        // input_tokens = 500 - 1000 = 0 (clamped to 0)
        // input_cost = 0 * 1 * 1e-6 = 0.0
        // cached_input_cost = 1000 * 0.5 * 1e-6 = 0.0005
        // cached_input_write_cost = 0 * 0.3 * 1e-6 = 0.0
        // output_cost = 200 * 2 * 1e-6 = 0.0004
        // total_cost = 0.0 + 0.0005 + 0.0 + 0.0004 = 0.0009

        assert_eq!(result.cost, 0.0009);
        assert_eq!(result.per_input_token, 1.0);
        assert_eq!(result.per_cached_input_token, Some(0.5));
        assert_eq!(result.per_cached_input_write_token, Some(0.3));
        assert_eq!(result.per_output_token, 2.0);
        assert!(!result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_edge_case_more_cached_write_than_input() {
        let usage = CompletionModelUsage {
            input_tokens: 500,
            output_tokens: 200,
            total_tokens: 700,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(300), Some(400), None)),
            completion_tokens_details: None,
            is_cache_used: false,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = Some(0.3);
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // More cached write tokens than remaining input tokens (edge case)
        // input_tokens = 500 - 300 - 400 = -200 (clamped to 0)
        // input_cost = 0 * 1 * 1e-6 = 0.0
        // cached_input_cost = 300 * 0.5 * 1e-6 = 0.00015
        // cached_input_write_cost = 400 * 0.3 * 1e-6 = 0.00012
        // output_cost = 200 * 2 * 1e-6 = 0.0004
        // total_cost = 0.0 + 0.00015 + 0.00012 + 0.0004 = 0.00067

        assert!((result.cost - 0.00067).abs() < 1e-10);
        assert_eq!(result.per_input_token, 1.0);
        assert_eq!(result.per_cached_input_token, Some(0.5));
        assert_eq!(result.per_cached_input_write_token, Some(0.3));
        assert_eq!(result.per_output_token, 2.0);
        assert!(!result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }

    #[test]
    fn test_calculate_tokens_cost_with_cache_no_cached_write_token_price() {
        let usage = CompletionModelUsage {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            prompt_tokens_details: Some(PromptTokensDetails::new(Some(300), Some(100), None)),
            completion_tokens_details: None,
            is_cache_used: true,
        };

        let cost_per_input_token = 1.0;
        let cost_per_cached_input_token = Some(0.5);
        let cost_per_cached_input_write_token = None; // No specific cached input write token price
        let cost_per_output_token = 2.0;

        let result = calculate_tokens_cost(
            &usage,
            cost_per_input_token,
            cost_per_cached_input_token,
            cost_per_cached_input_write_token,
            cost_per_output_token,
        );

        // When cache is used, prices are divided by 100
        // Since no cached input write token price is set, it uses input token price
        // input_tokens = 1000 - 300 - 100 = 600
        // input_cost = 600 * (1/100) * 1e-6 = 0.000006
        // cached_input_cost = 300 * (0.5/100) * 1e-6 = 0.0000015
        // cached_input_write_cost = 100 * (1/100) * 1e-6 = 0.000001 (uses input token price)
        // output_cost = 500 * (2/100) * 1e-6 = 0.00001
        // total_cost = 0.000006 + 0.0000015 + 0.000001 + 0.00001 = 0.0000185

        assert!((result.cost - 0.0000185).abs() < 1e-10);
        assert_eq!(result.per_input_token, 0.01); // 1 / 100
        assert_eq!(result.per_cached_input_token, Some(0.005)); // 0.5 / 100
        assert_eq!(result.per_cached_input_write_token, None);
        assert_eq!(result.per_output_token, 0.02); // 2 / 100
        assert!(result.is_cache_used);
        assert_eq!(result.per_image_cost, None);
    }
}
