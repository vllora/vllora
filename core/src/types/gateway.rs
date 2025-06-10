use crate::model::tools::Tool;
use crate::types::cache::ResponseCacheOptions;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Display;
use std::hash::Hash;
use thiserror::Error;

pub use async_openai::types::ResponseFormat as OpenaiResponseFormat;
pub use async_openai::types::ResponseFormatJsonSchema;

use super::engine::ModelTool;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatCompletionRequest {
    pub model: String,
    #[serde(default)]
    pub messages: Vec<ChatCompletionMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<async_openai::types::ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    // Keeping functions for backward compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<ChatCompletionFunction>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<Value>,
    // New tools API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatCompletionTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
}

impl ChatCompletionRequest {
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }
}

impl Hash for ChatCompletionRequest {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.messages.hash(state);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Thinking {
    pub r#type: String,
    pub budget_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extra {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<RequestUser>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<GuardOrName>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<ResponseCacheOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GuardOrName {
    GuardId(String),
    GuardWithParameters(GuardWithParameters),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardWithParameters {
    pub id: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatCompletionRequestWithTools<T> {
    #[serde(flatten)]
    pub request: ChatCompletionRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<McpDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router: Option<DynamicRouter<T>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<Extra>,
    #[serde(flatten)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_specific: Option<ProviderSpecificRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSpecificRequest {
    // Anthropic request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestUser {
    #[serde(alias = "user_id")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(alias = "user_name")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(alias = "user_tags", alias = "tags")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct DynamicRouter<T> {
    #[serde(flatten)]
    pub strategy: T,
    #[serde(default)]
    pub targets: Vec<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolsFilter {
    All,
    Selected(Vec<ToolSelector>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSelector {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum McpTransportType {
    Sse {
        server_url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        env: Option<HashMap<String, String>>,
    },
    Ws {
        server_url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        env: Option<HashMap<String, String>>,
    },
    Http {
        server_url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        env: Option<HashMap<String, String>>,
    },
    #[serde(rename = "in-memory")]
    InMemory {
        #[serde(default = "default_in_memory_name")]
        name: String,
    },
}

impl McpTransportType {
    pub fn key(&self) -> String {
        match self {
            McpTransportType::Sse { server_url, .. } => format!("sse:{}", server_url),
            McpTransportType::Ws { server_url, .. } => format!("ws:{}", server_url),
            McpTransportType::InMemory { name, .. } => format!("in-memory:{}", name),
            McpTransportType::Http { server_url, .. } => format!("http:{}", server_url),
        }
    }
}

fn default_in_memory_name() -> String {
    "langdb".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpDefinition {
    #[serde(default = "default_tools_filter")]
    pub filter: ToolsFilter,
    #[serde(flatten)]
    pub r#type: McpTransportType,
}

impl McpDefinition {
    pub fn server_name(&self) -> String {
        match &self.r#type {
            McpTransportType::InMemory { name, .. } => name.clone(),
            McpTransportType::Sse { server_url, .. } => server_url.clone(),
            McpTransportType::Ws { server_url, .. } => server_url.clone(),
            McpTransportType::Http { server_url, .. } => server_url.clone(),
        }
    }

    pub fn env(&self) -> Option<HashMap<String, String>> {
        match &self.r#type {
            McpTransportType::InMemory { .. } => None,
            McpTransportType::Sse { env, .. } => env.clone(),
            McpTransportType::Ws { env, .. } => env.clone(),
            McpTransportType::Http { env, .. } => env.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerTools {
    pub definition: McpDefinition,
    pub tools: Vec<McpTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool(pub rmcp::model::Tool, pub McpDefinition);

// Helper functions for serde defaults
fn default_tools_filter() -> ToolsFilter {
    ToolsFilter::All
}

impl From<McpTool> for ModelTool {
    fn from(val: McpTool) -> Self {
        ModelTool {
            name: val.name(),
            description: Some(val.description()),
            passed_args: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub response_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct InputAudio {
    pub data: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    ImageUrl,
    InputAudio,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct Content {
    pub r#type: ContentType,
    pub text: Option<String>,
    pub image_url: Option<ImageUrl>,
    pub audio: Option<InputAudio>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
#[serde(untagged)]
pub enum ChatCompletionContent {
    Text(String),
    Content(Vec<Content>),
}

impl ChatCompletionContent {
    pub fn as_string(&self) -> Option<String> {
        match self {
            ChatCompletionContent::Text(content) => Some(content.clone()),
            _ => None,
        }
    }
}

impl Default for ChatCompletionContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Hash, PartialEq, Eq)]
pub struct ChatCompletionMessage {
    pub role: String,
    pub content: Option<ChatCompletionContent>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub refusal: Option<String>,
    pub tool_call_id: Option<String>,
}

impl ChatCompletionMessage {
    pub fn new_text(role: String, content: String) -> Self {
        Self {
            role,
            content: Some(ChatCompletionContent::Text(content)),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Hash, PartialEq, Eq)]
pub struct ToolCall {
    pub index: Option<usize>,
    pub id: String,
    pub r#type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Hash, PartialEq, Eq)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatCompletionFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: FunctionParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ChatCompletionFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
    pub usage: ChatCompletionUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChoice {
    pub index: i32,
    pub message: ChatCompletionMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatCompletionUsage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatModel {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionParameters {
    pub r#type: String,
    pub properties: HashMap<String, Property>,
    pub required: Vec<String>,
}

impl Default for FunctionParameters {
    fn default() -> Self {
        Self {
            r#type: "object".to_owned(),
            properties: Default::default(),
            required: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    pub r#type: PropertyType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Property>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyType {
    Single(String),
    List(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatCompletionChunkChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub usage: Option<ChatCompletionUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunkChoice {
    pub index: i32,
    pub delta: ChatCompletionDelta,
    pub finish_reason: Option<String>,
    pub logprobs: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CompletionModelUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ImageGenerationModelUsage {
    pub quality: String,
    pub size: (u32, u32),
    pub images_count: u8,
    pub steps_count: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptTokensDetails {
    cached_tokens: u32,
    audio_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CompletionTokensDetails {
    accepted_prediction_tokens: u32,
    audio_tokens: u32,
    reasoning_tokens: u32,
    rejected_prediction_tokens: u32,
}

#[derive(Error, Debug)]
pub enum CostCalculatorError {
    #[error("Calcualtion error: {0}")]
    CalculationError(String),

    #[error("Model not found")]
    ModelNotFound,
}

#[derive(Serialize, Debug)]
pub struct CostCalculationResult {
    pub cost: f64,
    pub per_input_token: f64,
    pub per_output_token: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_image_cost: Option<ImageCostCalculationResult>,
}

#[derive(Serialize, Debug)]
pub enum ImageCostCalculationResult {
    TypePrice {
        size: String,
        quality: String,
        per_image: f64,
    },
    MPPrice(f64),
    SingleImagePrice(f64),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Usage {
    CompletionModelUsage(CompletionModelUsage),
    ImageGenerationModelUsage(ImageGenerationModelUsage),
}

#[async_trait::async_trait]
pub trait CostCalculator: Send + Sync {
    async fn calculate_cost(
        &self,
        model_name: &str,
        provider_name: &str,
        usage: &Usage,
    ) -> Result<CostCalculationResult, CostCalculatorError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Input {
    String(String),
    Array(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEmbeddingRequest {
    pub model: String,
    pub input: Input,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    pub dimensions: Option<u16>,
    #[serde(default)]
    pub encoding_format: EncodingFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum EncodingFormat {
    #[default]
    Float,
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateImageRequest {
    pub prompt: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<ImageQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ImageResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<ImageSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ImageStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageQuality {
    #[serde(rename = "standard")]
    SD,
    #[serde(rename = "hd")]
    HD,
}

impl Display for ImageQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageQuality::SD => write!(f, "standard"),
            ImageQuality::HD => write!(f, "hd"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "snake_case")]
pub enum ImageResponseFormat {
    B64Json,
    Url,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageSize {
    Size256x256,
    Size512x512,
    Size1024x1024,
    Size1792x1024,
    Size1024x1792,
    Other((u32, u32)),
}

impl From<ImageSize> for (u32, u32) {
    fn from(value: ImageSize) -> Self {
        match value {
            ImageSize::Size256x256 => (256, 256),
            ImageSize::Size512x512 => (512, 512),
            ImageSize::Size1024x1024 => (1024, 1024),
            ImageSize::Size1792x1024 => (1792, 1024),
            ImageSize::Size1024x1792 => (1024, 1792),
            ImageSize::Other(size) => size,
        }
    }
}

impl Display for ImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageSize::Size256x256 => write!(f, "256x256"),
            ImageSize::Size512x512 => write!(f, "512x512"),
            ImageSize::Size1024x1024 => write!(f, "1024x1024"),
            ImageSize::Size1792x1024 => write!(f, "1792x1024"),
            ImageSize::Size1024x1792 => write!(f, "1024x1792"),
            ImageSize::Other((width, height)) => write!(f, "{}x{}", width, height),
        }
    }
}

impl Serialize for ImageSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let str = self.to_string();
        serializer.serialize_str(&str)
    }
}

impl<'de> Deserialize<'de> for ImageSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "256x256" => Ok(ImageSize::Size256x256),
            "512x512" => Ok(ImageSize::Size512x512),
            "1024x1024" => Ok(ImageSize::Size1024x1024),
            "1792x1024" => Ok(ImageSize::Size1792x1024),
            "1024x1792" => Ok(ImageSize::Size1024x1792),
            s => {
                let parts: Vec<&str> = s.split('x').collect();
                if parts.len() != 2 {
                    return Err(serde::de::Error::custom(
                        "Invalid image size format. Expected {width}x{height}",
                    ));
                }
                let width = parts[0]
                    .parse::<u32>()
                    .map_err(|_| serde::de::Error::custom("Invalid width value"))?;
                let height = parts[1]
                    .parse::<u32>()
                    .map_err(|_| serde::de::Error::custom("Invalid height value"))?;
                Ok(ImageSize::Other((width, height)))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename = "snake_case")]
pub enum ImageStyle {
    #[serde(rename = "vivid")]
    Vivid,
    #[serde(rename = "natural")]
    Natural,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contents() {
        let content = ChatCompletionContent::Content(vec![
            Content {
                r#type: ContentType::Text,
                text: Some("Hello".to_string()),
                image_url: None,
                audio: None,
            },
            Content {
                r#type: ContentType::ImageUrl,
                text: None,
                image_url: Some(ImageUrl {
                    url: "https://example.com/image.jpg".to_string(),
                }),
                audio: None,
            },
            Content {
                r#type: ContentType::InputAudio,
                text: None,
                image_url: None,
                audio: Some(InputAudio {
                    data: "audio data".to_string(),
                    format: "mp3".to_string(),
                }),
            },
        ]);

        println!("{:?}", serde_json::to_string(&content).unwrap());
    }

    #[test]
    fn test_image_size_serialization() {
        // Test predefined sizes
        assert_eq!(
            serde_json::to_string(&ImageSize::Size256x256).unwrap(),
            r#""256x256""#
        );

        // Test custom size
        assert_eq!(
            serde_json::to_string(&ImageSize::Other((800, 600))).unwrap(),
            r#""800x600""#
        );
    }

    #[test]
    fn test_image_size_deserialization() {
        // Test predefined sizes
        assert_eq!(
            serde_json::from_str::<ImageSize>(r#""256x256""#).unwrap(),
            ImageSize::Size256x256
        );

        // Test custom size
        assert_eq!(
            serde_json::from_str::<ImageSize>(r#""800x600""#).unwrap(),
            ImageSize::Other((800, 600))
        );

        // Test invalid format
        assert!(serde_json::from_str::<ImageSize>(r#""invalid""#).is_err());
        assert!(serde_json::from_str::<ImageSize>(r#""800x""#).is_err());
        assert!(serde_json::from_str::<ImageSize>(r#""x600""#).is_err());
        assert!(serde_json::from_str::<ImageSize>(r#""axb""#).is_err());
    }

    #[test]
    fn deserialize_nested() {
        let json = r#"
            {
                "description": "2D array",
                "type": "array",
                "items": {
                    "type": "array",
                    "items": {
                        "type": ["string", "number", "boolean", "null"],
                        "description": "A single value"
                    }
                }
            }
        "#;
        let v: Property = serde_json::from_str(json).unwrap();

        println!("{v:#?}");

        let v = serde_json::to_string(&v).unwrap();
        println!("{v}");
    }
}
