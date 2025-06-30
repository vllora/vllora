use std::fmt::Display;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::engine::ModelType;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BedrockProvider {
    Cohere,
    Meta,
    Mistral,
    Other(String),
}
impl BedrockProvider {
    pub fn from_model_name(id: &str) -> Self {
        let split = id.split('.').collect::<Vec<&str>>();
        let provider = split[0].to_lowercase();
        Self::from(provider.clone())
    }
}

impl Display for BedrockProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BedrockProvider::Cohere => write!(f, "cohere"),
            BedrockProvider::Meta => write!(f, "meta"),
            BedrockProvider::Mistral => write!(f, "mistral"),
            BedrockProvider::Other(provider) => write!(f, "{provider}"),
        }
    }
}

impl From<String> for BedrockProvider {
    fn from(value: String) -> Self {
        match value.to_lowercase().as_str() {
            "cohere" => BedrockProvider::Cohere,
            "meta" => BedrockProvider::Meta,
            "mistral" => BedrockProvider::Mistral,
            _ => BedrockProvider::Other(value),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase", into = "String", from = "String")]
pub enum InferenceModelProvider {
    OpenAI,
    Anthropic,
    Gemini,
    Bedrock,
    Proxy(String),
}

impl From<String> for InferenceModelProvider {
    fn from(value: String) -> Self {
        match value.to_lowercase().as_str() {
            "openai" => InferenceModelProvider::OpenAI,
            "anthropic" => InferenceModelProvider::Anthropic,
            "gemini" => InferenceModelProvider::Gemini,
            "bedrock" => InferenceModelProvider::Bedrock,
            other => InferenceModelProvider::Proxy(other.to_string()),
        }
    }
}
impl From<InferenceModelProvider> for String {
    fn from(val: InferenceModelProvider) -> Self {
        match val {
            InferenceModelProvider::OpenAI => "openai".to_string(),
            InferenceModelProvider::Anthropic => "anthropic".to_string(),
            InferenceModelProvider::Gemini => "gemini".to_string(),
            InferenceModelProvider::Bedrock => "bedrock".to_string(),
            InferenceModelProvider::Proxy(other) => other,
        }
    }
}

impl std::fmt::Display for InferenceModelProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceModelProvider::OpenAI => write!(f, "openai"),
            InferenceModelProvider::Anthropic => write!(f, "anthropic"),
            InferenceModelProvider::Gemini => write!(f, "gemini"),
            InferenceModelProvider::Bedrock => write!(f, "bedrock"),
            InferenceModelProvider::Proxy(name) => write!(f, "{name}"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[serde(rename_all = "lowercase")]
pub enum ModelPrice {
    Completion(CompletionModelPrice),
    Embedding(EmbeddingModelPrice),
    ImageGeneration(ImageGenerationPrice),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelPrice {
    pub per_input_token: f64,
    pub valid_from: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionModelPrice {
    pub per_input_token: f64,
    pub per_output_token: f64,
    pub valid_from: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationPrice {
    pub type_prices: Option<HashMap<String, HashMap<String, f64>>>,
    pub mp_price: Option<f64>,
    pub valid_from: Option<NaiveDate>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AvailableModel {
    pub id: String,
    pub price: Option<ModelPrice>,
    pub details: Option<String>,
    pub model_type: Option<ModelType>,
    pub provider: InferenceModelProvider,
}
