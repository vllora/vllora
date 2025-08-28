use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::GatewayError;

// Bundle the local pricing JSON at compile-time
const PRICES_JSON: &str = include_str!("bedrock_prices.json");

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct SearchContextCostPerQuery {
    pub search_context_size_low: Option<f64>,
    pub search_context_size_medium: Option<f64>,
    pub search_context_size_high: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ModelPricingEntry {
    // Capacity
    pub max_tokens: Option<u32>,
    pub max_input_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,

    // Token costs
    pub input_cost_per_token: Option<f64>,
    pub output_cost_per_token: Option<f64>,
    pub output_cost_per_reasoning_token: Option<f64>,
    pub input_cost_per_token_batches: Option<f64>,
    pub output_cost_per_token_batches: Option<f64>,
    pub cache_read_input_token_cost: Option<f64>,

    // Image generation special cases
    pub output_cost_per_image: Option<f64>,

    // Misc priced features
    pub search_context_cost_per_query: Option<SearchContextCostPerQuery>,
    pub file_search_cost_per_1k_calls: Option<f64>,
    pub file_search_cost_per_gb_per_day: Option<f64>,
    pub vector_store_cost_per_gb_per_day: Option<f64>,
    pub computer_use_input_cost_per_1k_tokens: Option<f64>,
    pub computer_use_output_cost_per_1k_tokens: Option<f64>,
    pub code_interpreter_cost_per_session: Option<f64>,

    // Provider / metadata
    pub mode: Option<String>,
    pub supported_endpoints: Option<Vec<String>>,
    pub supported_modalities: Option<Vec<String>>,
    pub supported_output_modalities: Option<Vec<String>>,
    pub supported_regions: Option<Vec<String>>,
    pub deprecation_date: Option<String>,
    pub source: Option<String>,

    // Capabilities flags
    pub supports_function_calling: Option<bool>,
    pub supports_parallel_function_calling: Option<bool>,
    pub supports_vision: Option<bool>,
    pub supports_audio_input: Option<bool>,
    pub supports_audio_output: Option<bool>,
    pub supports_prompt_caching: Option<bool>,
    pub supports_response_schema: Option<bool>,
    pub supports_system_messages: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_web_search: Option<bool>,
    pub supports_pdf_input: Option<bool>,
    pub supports_tool_choice: Option<bool>,
    pub supports_native_streaming: Option<bool>,

    // Free-form metadata sometimes present
    pub metadata: Option<serde_json::Value>,
}

pub(crate) async fn fetch_pricing() -> Result<HashMap<String, ModelPricingEntry>, GatewayError> {
    let map: HashMap<String, ModelPricingEntry> = serde_json::from_str(PRICES_JSON)?;
    Ok(map)
}
