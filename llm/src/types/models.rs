use crate::types::engine::CustomInferenceApiType;
use crate::types::provider::CompletionModelPrice;
use crate::types::provider::InferenceModelProvider;
use crate::types::provider::ModelPrice;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Tools,
    Reasoning,
}

impl FromStr for ModelCapability {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tools" => Ok(ModelCapability::Tools),
            "reasoning" => Ok(ModelCapability::Reasoning),
            _ => Err(format!("Invalid ModelCapability: {s}")),
        }
    }
}

impl Display for ModelCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelCapability::Tools => write!(f, "tools"),
            ModelCapability::Reasoning => write!(f, "reasoning"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelIOFormats {
    Text,
    Image,
    Audio,
    Video,
}

impl FromStr for ModelIOFormats {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(ModelIOFormats::Text),
            "image" => Ok(ModelIOFormats::Image),
            "audio" => Ok(ModelIOFormats::Audio),
            "video" => Ok(ModelIOFormats::Video),
            _ => Err("Invalid ModelIOFormats".to_string()),
        }
    }
}

impl Display for ModelIOFormats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelIOFormats::Text => write!(f, "text"),
            ModelIOFormats::Image => write!(f, "image"),
            ModelIOFormats::Audio => write!(f, "audio"),
            ModelIOFormats::Video => write!(f, "video"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    Completions,
    Embeddings,
    ImageGeneration,
    Responses,
}

impl FromStr for ModelType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "completions" => Ok(ModelType::Completions),
            "embeddings" => Ok(ModelType::Embeddings),
            "image_generation" => Ok(ModelType::ImageGeneration),
            "responses" => Ok(ModelType::Responses),
            _ => Err(format!("Invalid ModelType: {s}")),
        }
    }
}

impl Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::Completions => write!(f, "completions"),
            ModelType::Embeddings => write!(f, "embeddings"),
            ModelType::ImageGeneration => write!(f, "image_generation"),
            ModelType::Responses => write!(f, "responses"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Limits {
    pub max_context_size: u32,
}

impl Limits {
    pub fn new(limit: u32) -> Self {
        Self {
            max_context_size: limit,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InferenceProvider {
    pub provider: InferenceModelProvider,
    pub model_name: String,
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_inference_api_type: Option<CustomInferenceApiType>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelMetadata {
    pub model: String,
    pub model_provider: String,
    pub inference_provider: InferenceProvider,
    pub price: ModelPrice,
    pub input_formats: Vec<ModelIOFormats>,
    pub output_formats: Vec<ModelIOFormats>,
    pub capabilities: Vec<ModelCapability>,
    pub r#type: ModelType,
    pub limits: Limits,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_info: Option<serde_json::Value>,
    #[serde(default)]
    pub virtual_model_id: Option<String>,
    #[serde(default)]
    pub min_service_level: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<chrono::NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_cutoff_date: Option<chrono::NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub langdb_release_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub is_private: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelMetadataWithEndpoints {
    #[serde(flatten)]
    pub model: ModelMetadata,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EndpointPricing {
    pub per_input_token: f64,
    pub per_output_token: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_cached_input_token: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_cached_input_write_token: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Endpoint {
    pub provider: InferenceProvider,
    pub available: bool,
    pub pricing: Option<EndpointPricing>,
}

impl Default for ModelMetadata {
    fn default() -> Self {
        Self {
            model: "".to_string(),
            model_provider: "".to_string(),
            inference_provider: InferenceProvider {
                provider: InferenceModelProvider::Proxy("vllora".to_string()),
                model_name: "".to_string(),
                endpoint: None,
                custom_inference_api_type: None,
            },
            price: ModelPrice::Completion(CompletionModelPrice {
                per_input_token: 0.0,
                per_output_token: 0.0,
                per_cached_input_token: None,
                per_cached_input_write_token: None,
                valid_from: None,
            }),
            input_formats: Vec::new(),
            output_formats: Vec::new(),
            capabilities: Vec::new(),
            r#type: ModelType::Completions,
            limits: Limits::new(0),
            description: "".to_string(),
            parameters: None,
            virtual_model_id: None,
            benchmark_info: None,
            min_service_level: 0,
            release_date: None,
            license: None,
            knowledge_cutoff_date: None,
            langdb_release_date: None,
            is_private: false,
        }
    }
}

impl ModelMetadata {
    pub fn qualified_model_name(&self) -> String {
        format!("{}/{}", self.inference_provider.provider, self.model)
    }
}

/// Groups models by name and creates endpoints for each model with availability based on credentials
pub fn group_models_by_name_with_endpoints(
    models: Vec<ModelMetadata>,
    provider_credentials_map: &HashMap<String, bool>,
) -> Vec<ModelMetadataWithEndpoints> {
    let mut grouped: HashMap<String, Vec<ModelMetadata>> = HashMap::new();

    // Group models by their model name
    for model in &models {
        grouped
            .entry(model.model.clone())
            .or_default()
            .push(model.clone());
    }

    // Convert grouped models to ModelMetadataWithEndpoints
    // Preserve database order by iterating through original models
    let mut result: Vec<ModelMetadataWithEndpoints> = Vec::new();
    let mut processed_models: std::collections::HashSet<String> = std::collections::HashSet::new();

    for model in models {
        if processed_models.contains(&model.model) {
            continue; // Skip if we already processed this model name
        }

        // Get all instances of this model
        if let Some(model_instances) = grouped.get(&model.model) {
            // Sort model instances by cost (cheapest first)
            let mut model_instances = model_instances.clone();
            model_instances.sort_by(|a, b| {
                let a_cost = a.price.per_input_token();
                let b_cost = b.price.per_input_token();
                a_cost
                    .partial_cmp(&b_cost)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Use the first (cheapest) model instance as the base
            let base_model = model_instances[0].clone();

            // Create endpoints from all instances with availability and pricing
            let endpoints: Vec<Endpoint> = model_instances
                .iter()
                .map(|model| {
                    // Check if provider has credentials configured using pre-fetched data
                    let provider_name =
                        model.inference_provider.provider.to_string().to_lowercase();
                    let has_credentials = provider_credentials_map
                        .get(&provider_name)
                        .copied()
                        .unwrap_or(false);

                    // Extract pricing from model
                    let pricing = match &model.price {
                        ModelPrice::Completion(price) => Some(EndpointPricing {
                            per_input_token: price.per_input_token,
                            per_output_token: price.per_output_token,
                            per_cached_input_token: price.per_cached_input_token,
                            per_cached_input_write_token: price.per_cached_input_write_token,
                        }),
                        ModelPrice::Embedding(price) => Some(EndpointPricing {
                            per_input_token: price.per_input_token,
                            per_output_token: 0.0,
                            per_cached_input_token: None,
                            per_cached_input_write_token: None,
                        }),
                        ModelPrice::ImageGeneration(_) => None,
                    };

                    Endpoint {
                        provider: model.inference_provider.clone(),
                        available: has_credentials,
                        pricing,
                    }
                })
                .collect();

            result.push(ModelMetadataWithEndpoints {
                model: base_model,
                endpoints,
            });

            processed_models.insert(model.model.clone());
        }
    }

    result
}
