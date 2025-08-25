use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::types::provider::{CompletionModelPrice, InferenceModelProvider, ModelPrice};

use std::str::FromStr;

/// OpenAI Completion Models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpenAICompletionModel {
    GPT35Turbo0125,
    GPT4o,
    GPT4oMini,
    O1Preview,
    O1Mini,
}

/// OpenAI Embedding Models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpenAIEmbeddingModel {
    Ada,
    EmbeddingSmall,
    EmbeddingLarge,
}

/// Gemini Completion Models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeminiCompletionModel {
    Gemini15Flash,
    Gemini15Flash8B,
    Gemini15Pro,
}

/// Anthropic Completion Models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnthropicCompletionModel {
    Claude3Opus20240229,
    Claude3Sonnet20240229,
    Claude3Haiku20240307,
    Claude35Sonnet20240620,
}

/// Bedrock Cohere Completion Models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BedrockCohereCompletionModel {
    CommandR,
    CommandRPlus,
}

/// Bedrock Llama Completion Models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BedrockMetaCompletionModel {
    Llama38BInstruct,
    Llama370BInstruct,
    Llama318BInstruct,
    Llama3170BInstruct,
    Llama321BInstruct,
    Llama323BInstruct,
    Llama3211BInstruct,
    Llama3370BInstruct,
}

/// Bedrock Mistral Completion Models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BedrockMistralCompletionModel {
    Mistral7BInstruct,
    Mistral8x7BInstruct,
}

impl Display for OpenAICompletionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenAICompletionModel::GPT35Turbo0125 => write!(f, "gpt-3.5-turbo-0125"),
            OpenAICompletionModel::GPT4o => write!(f, "gpt-4o"),
            OpenAICompletionModel::GPT4oMini => write!(f, "gpt-4o-mini"),
            OpenAICompletionModel::O1Preview => write!(f, "o1-preview"),
            OpenAICompletionModel::O1Mini => write!(f, "o1-mini"),
        }
    }
}

impl Display for OpenAIEmbeddingModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenAIEmbeddingModel::Ada => write!(f, "text-embedding-ada-002"),
            OpenAIEmbeddingModel::EmbeddingSmall => write!(f, "text-embedding-3-small"),
            OpenAIEmbeddingModel::EmbeddingLarge => write!(f, "text-embedding-3-large"),
        }
    }
}

impl Display for GeminiCompletionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeminiCompletionModel::Gemini15Flash => write!(f, "gemini-1.5-flash-latest"),
            GeminiCompletionModel::Gemini15Flash8B => write!(f, "gemini-1.5-flash-8b"),
            GeminiCompletionModel::Gemini15Pro => write!(f, "gemini-1.5-pro-latest"),
        }
    }
}

impl Display for AnthropicCompletionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnthropicCompletionModel::Claude3Opus20240229 => write!(f, "claude-3-opus-20240229"),
            AnthropicCompletionModel::Claude3Sonnet20240229 => {
                write!(f, "claude-3-sonnet-20240229")
            }
            AnthropicCompletionModel::Claude3Haiku20240307 => write!(f, "claude-3-haiku-20240307"),
            AnthropicCompletionModel::Claude35Sonnet20240620 => {
                write!(f, "claude-3-5-sonnet-20240620")
            }
        }
    }
}

impl Display for BedrockCohereCompletionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BedrockCohereCompletionModel::CommandR => write!(f, "command-r-v1:0"),
            BedrockCohereCompletionModel::CommandRPlus => write!(f, "command-r-plus-v1:0"),
        }
    }
}

impl Display for BedrockMetaCompletionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BedrockMetaCompletionModel::Llama370BInstruct => {
                write!(f, "llama3-70b-instruct-v1:0")
            }
            BedrockMetaCompletionModel::Llama38BInstruct => {
                write!(f, "llama3-8b-instruct-v1:0")
            }
            BedrockMetaCompletionModel::Llama318BInstruct => {
                write!(f, "llama3-1-8b-instruct-v1:0")
            }
            BedrockMetaCompletionModel::Llama3170BInstruct => {
                write!(f, "llama3-1-70b-instruct-v1:0")
            }
            BedrockMetaCompletionModel::Llama321BInstruct => {
                write!(f, "llama3-2-1b-instruct-v1:0")
            }
            BedrockMetaCompletionModel::Llama323BInstruct => {
                write!(f, "llama3-2-3b-instruct-v1:0")
            }
            BedrockMetaCompletionModel::Llama3211BInstruct => {
                write!(f, "llama3-2-11b-instruct-v1:0")
            }
            BedrockMetaCompletionModel::Llama3370BInstruct => {
                write!(f, "llama3-3-70b-instruct-v1:0")
            }
        }
    }
}

impl Display for BedrockMistralCompletionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BedrockMistralCompletionModel::Mistral7BInstruct => {
                write!(f, "mistral-7b-instruct-v0:2")
            }
            BedrockMistralCompletionModel::Mistral8x7BInstruct => {
                write!(f, "mixtral-8x7b-instruct-v0:1")
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Tools,
}

impl FromStr for ModelCapability {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tools" => Ok(ModelCapability::Tools),
            _ => Err("Invalid ModelCapability".to_string()),
        }
    }
}

impl Display for ModelCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelCapability::Tools => write!(f, "tools"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    Completions,
    Embeddings,
    ImageGeneration,
}

impl FromStr for ModelType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "completions" => Ok(ModelType::Completions),
            "embeddings" => Ok(ModelType::Embeddings),
            "image_generation" => Ok(ModelType::ImageGeneration),
            _ => Ok(ModelType::Completions),
        }
    }
}

impl Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::Completions => write!(f, "completions"),
            ModelType::Embeddings => write!(f, "embeddings"),
            ModelType::ImageGeneration => write!(f, "image_generation"),
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
    pub min_service_level: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<chrono::NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_cutoff_date: Option<chrono::NaiveDate>,
}

impl Default for ModelMetadata {
    fn default() -> Self {
        Self {
            model: "".to_string(),
            model_provider: "".to_string(),
            inference_provider: InferenceProvider {
                provider: InferenceModelProvider::Proxy("langdb".to_string()),
                model_name: "".to_string(),
                endpoint: None,
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
        }
    }
}

impl ModelMetadata {
    pub fn qualified_model_name(&self) -> String {
        format!("{}/{}", self.inference_provider.provider, self.model)
    }
}
