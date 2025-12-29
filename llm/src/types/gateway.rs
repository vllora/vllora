use crate::provider::gemini::types::Candidate;
use crate::provider::gemini::types::Content as GeminiContent;
use crate::types::cache::ResponseCacheOptions;
use crate::types::credentials_ident::CredentialsIdent;
use crate::types::provider::ModelPrice;
use crate::types::tools::ModelTool;
use crate::types::tools::Tool;
use crate::types::ModelFinishReason;
use crate::types::ToolCallExtra;
use async_openai::types::chat::ChatChoiceStream;
use async_openai::types::chat::ChatCompletionMessageToolCalls;
use async_openai::types::chat::ChatCompletionRequestMessage;
use async_openai::types::chat::ChatCompletionStreamResponseDelta;
use async_openai::types::chat::ChatCompletionToolChoiceOption;
use async_openai::types::chat::ChatCompletionTools;
use async_openai::types::chat::CreateChatCompletionRequest;
use async_openai::types::chat::CreateChatCompletionStreamResponse;
use async_openai::types::chat::FinishReason;
use async_openai::types::chat::ToolChoiceOptions;
use async_openai::types::embeddings::Base64EmbeddingVector;
use aws_sdk_bedrockruntime::types::TokenUsage;
use clust::messages::DeltaUsage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Display;
use std::hash::Hash;
use thiserror::Error;

pub use async_openai::types::chat::ResponseFormat as OpenaiResponseFormat;
pub use async_openai::types::chat::ResponseFormatJsonSchema;

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
    pub logit_bias: Option<HashMap<String, i8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<async_openai::types::chat::ResponseFormat>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
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

impl From<CreateChatCompletionRequest> for ChatCompletionRequest {
    fn from(request: CreateChatCompletionRequest) -> Self {
        ChatCompletionRequest {
            model: request.model,
            messages: request.messages.iter().map(map_message).collect(),
            temperature: request.temperature,
            top_p: request.top_p,
            n: request.n.map(|n| n as u32),
            stream: request.stream,
            stop: request.stop.map(|stop| match &stop {
                async_openai::types::chat::StopConfiguration::String(string) => {
                    vec![string.to_string()]
                }
                async_openai::types::chat::StopConfiguration::StringArray(string_array) => {
                    string_array.iter().map(|s| s.to_string()).collect()
                }
            }),
            #[allow(deprecated)]
            max_tokens: request.max_tokens,
            presence_penalty: request.presence_penalty,
            frequency_penalty: request.frequency_penalty,
            logit_bias: request.logit_bias,
            #[allow(deprecated)]
            user: request.user,
            response_format: request.response_format,
            #[allow(deprecated)]
            seed: request.seed,
            #[allow(deprecated)]
            functions: request.functions.map(|functions| {
                functions
                    .into_iter()
                    .map(|function| ChatCompletionFunction {
                        #[allow(deprecated)]
                        name: function.name.clone(),
                        #[allow(deprecated)]
                        description: function.description.clone(),
                        #[allow(deprecated)]
                        parameters: Some(function.parameters.clone()),
                    })
                    .collect()
            }),
            #[allow(deprecated)]
            function_call: request
                .function_call
                .map(|function_call| match function_call {
                    async_openai::types::chat::ChatCompletionFunctionCall::None => {
                        Value::String("none".to_string())
                    }
                    async_openai::types::chat::ChatCompletionFunctionCall::Auto => {
                        Value::String("auto".to_string())
                    }
                    async_openai::types::chat::ChatCompletionFunctionCall::Function { name } => {
                        let mut map = serde_json::Map::new();
                        map.insert("name".to_string(), Value::String(name.clone()));
                        Value::Object(map)
                    }
                }),
            tools: request.tools.map(|tools| {
                tools
                    .into_iter()
                    .map(|tool| ChatCompletionTool {
                        tool_type: "function".to_string(),
                        function: match tool {
                            ChatCompletionTools::Function(function) => ChatCompletionFunction {
                                name: function.function.name.clone(),
                                description: function.function.description.clone(),
                                parameters: function.function.parameters.clone(),
                            },
                            ChatCompletionTools::Custom(custom) => ChatCompletionFunction {
                                name: custom.custom.name.clone(),
                                description: custom.custom.description.clone(),
                                parameters: None,
                            },
                        },
                    })
                    .collect()
            }),
            tool_choice: request.tool_choice.map(|tool_choice| match tool_choice {
                ChatCompletionToolChoiceOption::AllowedTools(_tools) => {
                    todo!()
                }
                ChatCompletionToolChoiceOption::Function(function) => {
                    let mut function_map = serde_json::Map::new();
                    function_map.insert(
                        "name".to_string(),
                        Value::String(function.function.name.clone()),
                    );

                    let mut map = serde_json::Map::new();
                    map.insert("function".to_string(), Value::Object(function_map));

                    Value::Object(map)
                }
                ChatCompletionToolChoiceOption::Custom(custom) => {
                    let mut function_map = serde_json::Map::new();
                    function_map.insert(
                        "name".to_string(),
                        Value::String(custom.custom.name.clone()),
                    );

                    let mut map = serde_json::Map::new();
                    map.insert("function".to_string(), Value::Object(function_map));

                    Value::Object(map)
                }
                ChatCompletionToolChoiceOption::Mode(mode) => match mode {
                    ToolChoiceOptions::None => Value::String("none".to_string()),
                    ToolChoiceOptions::Auto => Value::String("auto".to_string()),
                    ToolChoiceOptions::Required => Value::String("required".to_string()),
                },
            }),
            stream_options: request.stream_options.map(|stream_options| StreamOptions {
                include_usage: stream_options.include_usage.unwrap_or(false),
            }),
            prompt_cache_key: request.prompt_cache_key,
        }
    }
}

fn map_message(message: &ChatCompletionRequestMessage) -> ChatCompletionMessage {
    match message {ChatCompletionRequestMessage::Developer(message) =>match &message.content {
            async_openai::types::chat::ChatCompletionRequestDeveloperMessageContent::Text(text) => ChatCompletionMessage {
                role: "developer".to_string(),
                content: Some(ChatCompletionContent::Text(text.clone())),
                ..Default::default()
            },
            async_openai::types::chat::ChatCompletionRequestDeveloperMessageContent::Array(array) => ChatCompletionMessage {
                role: "developer".to_string(),
                content: Some(ChatCompletionContent::Content(array.iter().map(|part| part.into()).collect())),
                ..Default::default()
            },
        }
        ChatCompletionRequestMessage::System(message) => match &message.content {
            async_openai::types::chat::ChatCompletionRequestSystemMessageContent::Text(text) => ChatCompletionMessage {
                role: "system".to_string(),
                content: Some(ChatCompletionContent::Text(text.clone())),
                ..Default::default()
            },
            async_openai::types::chat::ChatCompletionRequestSystemMessageContent::Array(array) => {
                let parts = array.iter().map(|part| match part {
                    async_openai::types::chat::ChatCompletionRequestSystemMessageContentPart::Text(text) => Content {
                        r#type: ContentType::Text,
                        text: Some(text.text.clone()),
                        ..Default::default()
                    },
                }).collect();
                ChatCompletionMessage {
                    role: "system".to_string(),
                    content: Some(ChatCompletionContent::Content(parts)),
                    ..Default::default()
                }
            }
        },
        ChatCompletionRequestMessage::User(message) => ChatCompletionMessage {
            role: "user".to_string(),
            content: match &message.content {
                async_openai::types::chat::ChatCompletionRequestUserMessageContent::Text(text) => Some(ChatCompletionContent::Text(text.clone())),
                async_openai::types::chat::ChatCompletionRequestUserMessageContent::Array(array) => Some(
                    ChatCompletionContent::Content(
                        array.iter().map(|part| {
                            match part {
                                async_openai::types::chat::ChatCompletionRequestUserMessageContentPart::Text(text) => Content {
                                    r#type: ContentType::Text,
                                    text: Some(text.text.clone()),
                                    ..Default::default()
                                },
                                async_openai::types::chat::ChatCompletionRequestUserMessageContentPart::ImageUrl(image_url) => Content {
                                    r#type: ContentType::ImageUrl,
                                    image_url: Some(ImageUrl {
                                        url: image_url.image_url.url.clone(),
                                    }),
                                    ..Default::default()
                                },
                                async_openai::types::chat::ChatCompletionRequestUserMessageContentPart::InputAudio(input_audio) => Content {
                                    r#type: ContentType::InputAudio,
                                    audio: Some(InputAudio {
                                        data: input_audio.input_audio.data.clone(),
                                        format: match input_audio.input_audio.format {
                                            async_openai::types::chat::InputAudioFormat::Mp3 => "mp3".to_string(),
                                            async_openai::types::chat::InputAudioFormat::Wav => "wav".to_string(),
                                        },
                                    }),
                                    ..Default::default()
                                },
                                async_openai::types::chat::ChatCompletionRequestUserMessageContentPart::File(file) => Content {
                                    r#type: ContentType::File,
                                    file: Some(File {
                                        data: file.file.file_data.clone(),
                                        id: file.file.file_id.clone(),
                                        filename: file.file.filename.clone(),
                                    }),
                                    ..Default::default()
                                },
                            }
                        }).collect()
                    )
                )
            },
            ..Default::default()
        },
        ChatCompletionRequestMessage::Assistant(message) => ChatCompletionMessage {
            role: "assistant".to_string(),
            content: match &message.content {
                Some(async_openai::types::chat::ChatCompletionRequestAssistantMessageContent::Text(text)) => Some(ChatCompletionContent::Text(text.clone())),
                Some(async_openai::types::chat::ChatCompletionRequestAssistantMessageContent::Array(array)) => Some(
                    ChatCompletionContent::Content(array.iter().map(|part| {
                        match part {
                            async_openai::types::chat::ChatCompletionRequestAssistantMessageContentPart::Text(text) => Content {
                                r#type: ContentType::Text,
                                text: Some(text.text.clone()),
                                ..Default::default()
                            },
                            async_openai::types::chat::ChatCompletionRequestAssistantMessageContentPart::Refusal(refusal) => Content {
                                r#type: ContentType::Text,
                                text: Some(refusal.refusal.clone()),
                                ..Default::default()
                            },
                        }
                    }).collect())
                ),
                None => None,
            },
            refusal: message.refusal.clone(),
            tool_calls: message.tool_calls.as_ref().map(|tool_calls| tool_calls.iter().enumerate().map(|(index, tool_call)| {
                let mut function_call: ToolCall = tool_call.into();
                function_call.index = Some(index);

                function_call
            }).collect()),
            tool_call_id: message.tool_calls.as_ref().and_then(|tool_calls| tool_calls.first().map(|tool_call| {
                match tool_call {
                    ChatCompletionMessageToolCalls::Function(function) => function.id.clone(),
                    ChatCompletionMessageToolCalls::Custom(custom) => custom.id.clone(),
                }
            })),
            cache_control: None
        },
        ChatCompletionRequestMessage::Tool(message) => ChatCompletionMessage {
            role: "tool".to_string(),
            tool_call_id: Some(message.tool_call_id.clone()),
            content: Some(match &message.content {
                async_openai::types::chat::ChatCompletionRequestToolMessageContent::Text(text) => ChatCompletionContent::Text(text.clone()),
                async_openai::types::chat::ChatCompletionRequestToolMessageContent::Array(array) => ChatCompletionContent::Content(array.iter().map(|part| match part {
                        async_openai::types::chat::ChatCompletionRequestToolMessageContentPart::Text(text) => Content {
                            r#type: ContentType::Text,
                            text: Some(text.text.clone()),
                            ..Default::default()
                        }
                    }
                ).collect()),
            }),
            ..Default::default()
        },
        ChatCompletionRequestMessage::Function(message) => ChatCompletionMessage {
            role: "function".to_string(),
            content: Some(ChatCompletionContent::Text(message.content.clone().unwrap_or_default())),
            ..Default::default()
        },
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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<HashMap<String, serde_json::Value>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelNameOrTarget {
    ModelName(String),
    Target(HashMap<String, serde_json::Value>),
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
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<Extra>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallbacks: Option<Vec<ModelNameOrTarget>>,
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
    #[serde(alias = "user_email")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
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
    #[serde(rename = "in-memory", alias = "memory")]
    InMemory {
        #[serde(default = "default_in_memory_name")]
        name: String,
    },
}

impl McpTransportType {
    pub fn key(&self) -> String {
        match self {
            McpTransportType::Sse { server_url, .. } => format!("sse:{server_url}"),
            McpTransportType::Ws { server_url, .. } => format!("ws:{server_url}"),
            McpTransportType::InMemory { name, .. } => format!("in-memory:{name}"),
            McpTransportType::Http { server_url, .. } => format!("http:{server_url}"),
        }
    }
}

fn default_in_memory_name() -> String {
    "vllora".to_string()
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

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct File {
    pub data: Option<String>,
    pub id: Option<String>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    #[default]
    Text,
    ImageUrl,
    InputAudio,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq, Default)]
pub struct Content {
    pub r#type: ContentType,
    pub text: Option<String>,
    pub image_url: Option<ImageUrl>,
    pub audio: Option<InputAudio>,
    pub cache_control: Option<CacheControl>,
    pub file: Option<File>,
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
            ChatCompletionContent::Content(content) => content
                .iter()
                .find(|c| c.r#type == ContentType::Text)
                .and_then(|c| c.text.clone()),
        }
    }

    pub fn as_content(&self) -> Option<Vec<Content>> {
        match self {
            ChatCompletionContent::Content(content) => Some(content.clone()),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ChatCompletionContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatCompletionMessageWithFinishReason {
    id: String,
    created: u32,
    model: String,
    message: ChatCompletionMessage,
    finish_reason: ModelFinishReason,
    usage: Option<GatewayModelUsage>,
}

impl ChatCompletionMessageWithFinishReason {
    pub fn new(
        message: ChatCompletionMessage,
        finish_reason: ModelFinishReason,
        id: String,
        created: u32,
        model: String,
        usage: Option<GatewayModelUsage>,
    ) -> Self {
        Self {
            message,
            finish_reason,
            id,
            created,
            model,
            usage,
        }
    }

    pub fn finish_reason(&self) -> &ModelFinishReason {
        &self.finish_reason
    }

    pub fn message(&self) -> &ChatCompletionMessage {
        &self.message
    }
}

impl From<ChatCompletionMessageWithFinishReason>
    for async_openai::types::chat::CreateChatCompletionResponse
{
    fn from(
        val: ChatCompletionMessageWithFinishReason,
    ) -> async_openai::types::chat::CreateChatCompletionResponse {
        let usage = val
            .usage
            .as_ref()
            .map(|usage| async_openai::types::chat::CompletionUsage {
                prompt_tokens: usage.input_tokens,
                completion_tokens: usage.output_tokens,
                total_tokens: usage.total_tokens,
                prompt_tokens_details: usage
                    .prompt_tokens_details
                    .as_ref()
                    .map(|details| details.clone().into()),
                completion_tokens_details: usage
                    .completion_tokens_details
                    .as_ref()
                    .map(|details| details.clone().into()),
            });

        async_openai::types::chat::CreateChatCompletionResponse {
            id: val.id,
            object: Some("chat.completion".to_string()),
            created: val.created,
            model: val.model,
            choices: vec![async_openai::types::chat::ChatChoice {
                index: 0,
                message: async_openai::types::chat::ChatCompletionResponseMessage {
                    content: Some(val.message.content.unwrap().as_string().unwrap()),
                    role: match val.message.role.as_str() {
                        "assistant" => async_openai::types::chat::Role::Assistant,
                        "system" => async_openai::types::chat::Role::System,
                        "tool" => async_openai::types::chat::Role::Tool,
                        _ => async_openai::types::chat::Role::User,
                    },
                    tool_calls: val.message.tool_calls.as_ref().map(|tool_calls| {
                        tool_calls
                            .iter()
                            .map(|tool_call| {
                                ChatCompletionMessageToolCalls::Function(tool_call.clone().into())
                            })
                            .collect::<Vec<ChatCompletionMessageToolCalls>>()
                    }),
                    refusal: val.message.refusal.clone(),
                    #[allow(deprecated)]
                    function_call: None,
                    audio: None,
                    annotations: None,
                },
                finish_reason: Some(val.finish_reason.into()),
                logprobs: None,
            }],
            usage,
            service_tier: None,
            #[allow(deprecated)]
            system_fingerprint: None,
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_content: Option<ToolCallExtra>,
}

impl From<ToolCall> for async_openai::types::chat::ChatCompletionMessageToolCall {
    fn from(val: ToolCall) -> async_openai::types::chat::ChatCompletionMessageToolCall {
        async_openai::types::chat::ChatCompletionMessageToolCall {
            function: val.function.into(),
            id: val.id,
        }
    }
}

impl From<async_openai::types::chat::ChatCompletionMessageToolCalls> for ToolCall {
    fn from(val: async_openai::types::chat::ChatCompletionMessageToolCalls) -> Self {
        match val {
            ChatCompletionMessageToolCalls::Function(function) => ToolCall {
                index: None,
                id: function.id,
                r#type: "function".to_string(),
                function: function.function.into(),
                extra_content: None,
            },
            ChatCompletionMessageToolCalls::Custom(custom) => ToolCall {
                index: None,
                id: custom.id,
                r#type: "tool".to_string(),
                function: custom.custom_tool.into(),
                extra_content: None,
            },
        }
    }
}

impl From<&async_openai::types::chat::ChatCompletionMessageToolCalls> for ToolCall {
    fn from(val: &async_openai::types::chat::ChatCompletionMessageToolCalls) -> Self {
        val.clone().into()
    }
}

impl From<async_openai::types::chat::CustomTool> for FunctionCall {
    fn from(val: async_openai::types::chat::CustomTool) -> Self {
        FunctionCall {
            name: val.name,
            arguments: val.input,
        }
    }
}

impl From<&async_openai::types::chat::ChatCompletionMessageToolCall> for ToolCall {
    fn from(val: &async_openai::types::chat::ChatCompletionMessageToolCall) -> Self {
        val.clone().into()
    }
}

impl From<async_openai::types::chat::ChatCompletionMessageToolCall> for ToolCall {
    fn from(val: async_openai::types::chat::ChatCompletionMessageToolCall) -> Self {
        ToolCall {
            index: None,
            id: val.id,
            r#type: "function".to_string(),
            function: val.function.into(),
            extra_content: None,
        }
    }
}

impl From<&async_openai::types::chat::ChatCompletionMessageToolCallChunk> for ToolCall {
    fn from(val: &async_openai::types::chat::ChatCompletionMessageToolCallChunk) -> Self {
        val.clone().into()
    }
}

impl From<async_openai::types::chat::ChatCompletionMessageToolCallChunk> for ToolCall {
    fn from(val: async_openai::types::chat::ChatCompletionMessageToolCallChunk) -> Self {
        ToolCall {
            index: Some(val.index as usize),
            id: val.id.unwrap_or_default(),
            r#type: "function".to_string(),
            function: val.function.map(|f| f.into()).unwrap_or_default(),
            extra_content: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Hash, PartialEq, Eq)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

impl From<FunctionCall> for async_openai::types::chat::FunctionCall {
    fn from(val: FunctionCall) -> async_openai::types::chat::FunctionCall {
        async_openai::types::chat::FunctionCall {
            name: val.name,
            arguments: val.arguments,
        }
    }
}

impl From<async_openai::types::chat::FunctionCall> for FunctionCall {
    fn from(val: async_openai::types::chat::FunctionCall) -> Self {
        FunctionCall {
            name: val.name,
            arguments: val.arguments,
        }
    }
}

impl From<async_openai::types::chat::FunctionCallStream> for FunctionCall {
    fn from(val: async_openai::types::chat::FunctionCallStream) -> Self {
        FunctionCall {
            name: val.name.unwrap_or_default(),
            arguments: val.arguments.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatCompletionFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<serde_json::Value>,
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
    #[serde(skip_serializing)]
    pub is_cache_used: Option<bool>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
    pub cost: f64,
}

impl From<async_openai::types::chat::CompletionUsage> for ChatCompletionUsage {
    fn from(val: async_openai::types::chat::CompletionUsage) -> Self {
        ChatCompletionUsage {
            prompt_tokens: val.prompt_tokens as i32,
            completion_tokens: val.completion_tokens as i32,
            total_tokens: val.total_tokens as i32,
            prompt_tokens_details: val.prompt_tokens_details.map(|p| p.into()),
            completion_tokens_details: val.completion_tokens_details.map(|c| c.into()),
            cost: 0.0,
        }
    }
}

impl From<crate::provider::gemini::types::UsageMetadata> for ChatCompletionUsage {
    fn from(val: crate::provider::gemini::types::UsageMetadata) -> Self {
        ChatCompletionUsage {
            prompt_tokens: val.prompt_token_count as i32,
            completion_tokens: val.candidates_token_count.unwrap_or(0) as i32
                + val.thoughts_token_count.unwrap_or(0) as i32,
            total_tokens: val.total_token_count as i32,
            prompt_tokens_details: None,
            completion_tokens_details: val.thoughts_token_count.as_ref().map(|t| {
                CompletionTokensDetails {
                    reasoning_tokens: *t,
                    ..Default::default()
                }
            }),
            cost: 0.0,
        }
    }
}

impl From<TokenUsage> for ChatCompletionUsage {
    fn from(val: TokenUsage) -> Self {
        ChatCompletionUsage {
            prompt_tokens: val.input_tokens,
            completion_tokens: val.output_tokens,
            total_tokens: val.total_tokens,
            prompt_tokens_details: Some(PromptTokensDetails::new(
                val.cache_read_input_tokens.map(|t| t as u32),
                val.cache_write_input_tokens.map(|t| t as u32),
                Some(0),
            )),
            completion_tokens_details: None,
            cost: 0.0,
        }
    }
}

impl From<async_openai::types::chat::ChatCompletionRequestDeveloperMessageContentPart> for Content {
    fn from(
        val: async_openai::types::chat::ChatCompletionRequestDeveloperMessageContentPart,
    ) -> Self {
        match val {
            async_openai::types::chat::ChatCompletionRequestDeveloperMessageContentPart::Text(
                text,
            ) => Content {
                r#type: ContentType::Text,
                text: Some(text.text.clone()),
                ..Default::default()
            },
        }
    }
}

impl From<&async_openai::types::chat::ChatCompletionRequestDeveloperMessageContentPart>
    for Content
{
    fn from(
        val: &async_openai::types::chat::ChatCompletionRequestDeveloperMessageContentPart,
    ) -> Self {
        val.clone().into()
    }
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
    pub required: Option<Vec<String>>,
}

impl Default for FunctionParameters {
    fn default() -> Self {
        Self {
            r#type: "object".to_owned(),
            properties: Default::default(),
            required: Some(vec![]),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<PropertyType>,
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

impl From<clust::messages::Usage> for ChatCompletionUsage {
    fn from(usage: clust::messages::Usage) -> Self {
        let input_tokens = usage.input_tokens
            + usage.cache_read_input_tokens.unwrap_or(0)
            + usage.cache_creation_input_tokens.unwrap_or(0);

        ChatCompletionUsage {
            prompt_tokens: input_tokens as i32,
            completion_tokens: usage.output_tokens as i32,
            total_tokens: (input_tokens + usage.output_tokens) as i32,
            prompt_tokens_details: Some(PromptTokensDetails::new(
                usage.cache_read_input_tokens,
                usage.cache_creation_input_tokens,
                None,
            )),
            completion_tokens_details: None,
            cost: 0.0,
        }
    }
}

impl From<DeltaUsage> for ChatCompletionUsage {
    fn from(val: DeltaUsage) -> Self {
        ChatCompletionUsage {
            prompt_tokens: 0,
            completion_tokens: val.output_tokens as i32,
            total_tokens: val.output_tokens as i32,
            prompt_tokens_details: None,
            completion_tokens_details: None,
            cost: 0.0,
        }
    }
}

impl From<CreateChatCompletionStreamResponse> for ChatCompletionChunk {
    fn from(val: CreateChatCompletionStreamResponse) -> Self {
        ChatCompletionChunk {
            id: val.id,
            object: val.object.unwrap_or("chat.completion.chunk".to_string()),
            created: val.created as i64,
            model: val.model,
            choices: val.choices.into_iter().map(|c| c.into()).collect(),
            usage: val.usage.map(|u| u.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunkChoice {
    pub index: i32,
    pub delta: ChatCompletionDelta,
    pub finish_reason: Option<String>,
    pub logprobs: Option<async_openai::types::chat::ChatChoiceLogprobs>,
}

impl From<ChatChoiceStream> for ChatCompletionChunkChoice {
    fn from(val: ChatChoiceStream) -> Self {
        ChatCompletionChunkChoice {
            index: val.index as i32,
            delta: val.delta.into(),
            finish_reason: val.finish_reason.map(|f| {
                match f {
                    FinishReason::Stop => "stop",
                    FinishReason::Length => "length",
                    FinishReason::ToolCalls => "tool_calls",
                    FinishReason::ContentFilter => "content_filter",
                    FinishReason::FunctionCall => "function_call",
                }
                .to_string()
            }),
            logprobs: val.logprobs,
        }
    }
}

impl From<Candidate> for ChatCompletionChunkChoice {
    fn from(val: Candidate) -> Self {
        ChatCompletionChunkChoice {
            index: 0,
            delta: val.content.into(),
            finish_reason: val.finish_reason.as_ref().map(|f| {
                let reason =
                    crate::provider::gemini::model::GeminiModel::map_finish_reason(f, false);
                reason.to_string()
            }),
            logprobs: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatCompletionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatCompletionDelta {
    pub fn from_assistant_text(text: String) -> Self {
        ChatCompletionDelta {
            role: Some("assistant".to_string()),
            content: Some(text),
            tool_calls: None,
        }
    }

    pub fn from_tool_use(tool_call: ToolCall) -> Self {
        ChatCompletionDelta {
            role: Some("tool".to_string()),
            content: None,
            tool_calls: Some(vec![tool_call]),
        }
    }
}

impl From<ChatCompletionStreamResponseDelta> for ChatCompletionDelta {
    fn from(val: ChatCompletionStreamResponseDelta) -> Self {
        ChatCompletionDelta {
            role: val.role.map(|r| r.to_string()),
            content: val.content.map(|c| c.to_string()),
            tool_calls: val
                .tool_calls
                .map(|t| t.into_iter().map(|t| t.into()).collect()),
        }
    }
}

impl From<GeminiContent> for ChatCompletionDelta {
    fn from(val: GeminiContent) -> Self {
        let mut tool_calls: Option<Vec<ToolCall>> = None;
        let mut text = None;
        let mut contents: Vec<Content> = vec![];
        for part in val.parts {
            let signature = part.thought_signature.clone();
            match part.part {
                crate::provider::gemini::types::Part::FunctionCall { name, args } => {
                    let tool_call = ToolCall {
                        id: name.clone(),
                        function: FunctionCall {
                            name: name.clone(),
                            arguments: serde_json::to_string(&args).unwrap(),
                        },
                        extra_content: signature.as_ref().map(|s| ToolCallExtra {
                            google: Some(crate::types::GoogleToolCallExtra {
                                thought_signature: s.clone(),
                            }),
                        }),
                        index: None,
                        r#type: "function".to_string(),
                    };

                    if let Some(tool_calls) = &mut tool_calls {
                        tool_calls.push(tool_call);
                    } else {
                        tool_calls = Some(vec![tool_call]);
                    }
                }
                crate::provider::gemini::types::Part::Text(part_text) => {
                    text = Some(part_text.clone());
                }
                crate::provider::gemini::types::Part::InlineData { mime_type, data } => {
                    if mime_type.starts_with("audio/") {
                        contents.push(Content {
                            r#type: ContentType::InputAudio,
                            audio: Some(InputAudio {
                                data: data.clone(),
                                format: mime_type.clone(),
                            }),
                            cache_control: None,
                            text: None,
                            image_url: None,
                            file: None,
                        });
                    } else {
                        unreachable!("Unexpected mime type: {mime_type}");
                    }
                }
                crate::provider::gemini::types::Part::FileData {
                    mime_type,
                    file_uri,
                } => {
                    if mime_type.starts_with("image/") {
                        contents.push(Content {
                            r#type: ContentType::ImageUrl,
                            image_url: Some(ImageUrl {
                                url: file_uri.clone(),
                            }),
                            cache_control: None,
                            text: None,
                            audio: None,
                            file: None,
                        });
                    } else {
                        unreachable!("Unexpected mime type: {mime_type}");
                    }
                }
                _ => {
                    unreachable!("Unexpected part: {part:?}");
                }
            }
        }

        let content = if contents.is_empty() {
            text
        } else {
            unreachable!("Unexpected content: {contents:?}");
            // if let Some(text) = text {
            //     contents.push(Content {
            //         r#type: ContentType::Text,
            //         text: Some(text),
            //         cache_control: None,
            //         image_url: None,
            //         audio: None,
            //     });
            // }
            // Some(ChatCompletionContent::Content(contents))
        };

        ChatCompletionDelta {
            role: Some(match val.role {
                crate::provider::gemini::types::Role::User => "user".to_string(),
                crate::provider::gemini::types::Role::Model => "assistant".to_string(),
            }),
            content,
            tool_calls,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

/// Unified usage model for all models
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct GatewayModelUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
    pub is_cache_used: bool,
}

impl From<&async_openai::types::chat::CompletionUsage> for GatewayModelUsage {
    fn from(val: &async_openai::types::chat::CompletionUsage) -> Self {
        GatewayModelUsage {
            input_tokens: val.prompt_tokens,
            output_tokens: val.completion_tokens
                + val
                    .completion_tokens_details
                    .as_ref()
                    .and_then(|c| c.reasoning_tokens)
                    .unwrap_or(0),
            total_tokens: val.total_tokens,
            prompt_tokens_details: val.prompt_tokens_details.as_ref().map(|p| {
                crate::types::gateway::PromptTokensDetails::new(
                    p.cached_tokens,
                    Some(0),
                    p.audio_tokens,
                )
            }),
            completion_tokens_details: val.completion_tokens_details.as_ref().map(|c| {
                crate::types::gateway::CompletionTokensDetails::new(
                    c.accepted_prediction_tokens,
                    c.audio_tokens,
                    c.reasoning_tokens,
                    c.rejected_prediction_tokens,
                )
            }),
            ..Default::default()
        }
    }
}

impl From<async_openai::types::chat::CompletionUsage> for GatewayModelUsage {
    fn from(val: async_openai::types::chat::CompletionUsage) -> Self {
        GatewayModelUsage::from(&val)
    }
}

impl From<&async_openai::types::responses::ResponseUsage> for GatewayModelUsage {
    fn from(val: &async_openai::types::responses::ResponseUsage) -> Self {
        GatewayModelUsage {
            input_tokens: val.input_tokens,
            output_tokens: val.output_tokens,
            total_tokens: val.total_tokens,
            prompt_tokens_details: Some(PromptTokensDetails {
                cached_tokens: val.input_tokens_details.cached_tokens,
                cache_creation_tokens: 0,
                audio_tokens: 0,
            }),
            completion_tokens_details: Some(CompletionTokensDetails {
                accepted_prediction_tokens: 0,
                audio_tokens: 0,
                reasoning_tokens: val.output_tokens_details.reasoning_tokens,
                rejected_prediction_tokens: 0,
            }),
            ..Default::default()
        }
    }
}

impl From<async_openai::types::responses::ResponseUsage> for GatewayModelUsage {
    fn from(val: async_openai::types::responses::ResponseUsage) -> Self {
        GatewayModelUsage::from(&val)
    }
}

impl GatewayModelUsage {
    pub fn add_usage(&mut self, other: &Self) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.total_tokens += other.total_tokens;
        self.prompt_tokens_details = match (
            self.prompt_tokens_details.as_ref(),
            other.prompt_tokens_details.as_ref(),
        ) {
            (Some(p1), Some(p2)) => {
                let mut p1 = p1.clone();
                p1.add_usage(p2);
                Some(p1)
            }
            (Some(p), None) => Some(p.clone()),
            (None, Some(p)) => Some(p.clone()),
            (None, None) => None,
        };
        self.completion_tokens_details = match (
            self.completion_tokens_details.as_ref(),
            other.completion_tokens_details.as_ref(),
        ) {
            (Some(c1), Some(c2)) => {
                let mut c1 = c1.clone();
                c1.add_usage(c2);
                Some(c1)
            }
            (Some(c), None) => Some(c.clone()),
            (None, Some(c)) => Some(c.clone()),
            (None, None) => None,
        };
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ImageGenerationModelUsage {
    pub quality: String,
    pub size: (u32, u32),
    pub images_count: u8,
    pub steps_count: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct PromptTokensDetails {
    cached_tokens: u32,
    cache_creation_tokens: u32,
    audio_tokens: u32,
}

impl From<async_openai::types::chat::PromptTokensDetails> for PromptTokensDetails {
    fn from(val: async_openai::types::chat::PromptTokensDetails) -> Self {
        PromptTokensDetails::from(&val)
    }
}

impl From<&async_openai::types::chat::PromptTokensDetails> for PromptTokensDetails {
    fn from(val: &async_openai::types::chat::PromptTokensDetails) -> Self {
        PromptTokensDetails {
            cached_tokens: val.cached_tokens.unwrap_or(0),
            cache_creation_tokens: 0,
            audio_tokens: val.audio_tokens.unwrap_or(0),
        }
    }
}

impl From<PromptTokensDetails> for async_openai::types::chat::PromptTokensDetails {
    fn from(val: PromptTokensDetails) -> async_openai::types::chat::PromptTokensDetails {
        async_openai::types::chat::PromptTokensDetails {
            cached_tokens: Some(val.cached_tokens),
            audio_tokens: Some(val.audio_tokens),
        }
    }
}

impl From<CompletionTokensDetails> for async_openai::types::chat::CompletionTokensDetails {
    fn from(val: CompletionTokensDetails) -> async_openai::types::chat::CompletionTokensDetails {
        async_openai::types::chat::CompletionTokensDetails {
            accepted_prediction_tokens: Some(val.accepted_prediction_tokens),
            audio_tokens: Some(val.audio_tokens),
            reasoning_tokens: Some(val.reasoning_tokens),
            rejected_prediction_tokens: Some(val.rejected_prediction_tokens),
        }
    }
}

impl From<async_openai::types::chat::CompletionTokensDetails> for CompletionTokensDetails {
    fn from(val: async_openai::types::chat::CompletionTokensDetails) -> Self {
        CompletionTokensDetails::from(&val)
    }
}

impl From<&async_openai::types::chat::CompletionTokensDetails> for CompletionTokensDetails {
    fn from(val: &async_openai::types::chat::CompletionTokensDetails) -> Self {
        CompletionTokensDetails {
            accepted_prediction_tokens: val.accepted_prediction_tokens.unwrap_or(0),
            audio_tokens: val.audio_tokens.unwrap_or(0),
            reasoning_tokens: val.reasoning_tokens.unwrap_or(0),
            rejected_prediction_tokens: val.rejected_prediction_tokens.unwrap_or(0),
        }
    }
}

impl PromptTokensDetails {
    pub fn new(
        cached_tokens: Option<u32>,
        cache_creation_tokens: Option<u32>,
        audio_tokens: Option<u32>,
    ) -> Self {
        Self {
            cached_tokens: cached_tokens.unwrap_or(0),
            cache_creation_tokens: cache_creation_tokens.unwrap_or(0),
            audio_tokens: audio_tokens.unwrap_or(0),
        }
    }

    pub fn add_usage(&mut self, other: &Self) {
        self.cached_tokens += other.cached_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
        self.audio_tokens += other.audio_tokens;
    }

    pub fn cached_tokens(&self) -> u32 {
        self.cached_tokens
    }

    pub fn cache_creation_tokens(&self) -> u32 {
        self.cache_creation_tokens
    }

    pub fn audio_tokens(&self) -> u32 {
        self.audio_tokens
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct CompletionTokensDetails {
    accepted_prediction_tokens: u32,
    audio_tokens: u32,
    reasoning_tokens: u32,
    rejected_prediction_tokens: u32,
}

impl CompletionTokensDetails {
    pub fn new(
        accepted_prediction_tokens: Option<u32>,
        audio_tokens: Option<u32>,
        reasoning_tokens: Option<u32>,
        rejected_prediction_tokens: Option<u32>,
    ) -> Self {
        Self {
            accepted_prediction_tokens: accepted_prediction_tokens.unwrap_or(0),
            audio_tokens: audio_tokens.unwrap_or(0),
            reasoning_tokens: reasoning_tokens.unwrap_or(0),
            rejected_prediction_tokens: rejected_prediction_tokens.unwrap_or(0),
        }
    }

    pub fn add_usage(&mut self, other: &Self) {
        self.accepted_prediction_tokens += other.accepted_prediction_tokens;
        self.audio_tokens += other.audio_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
        self.rejected_prediction_tokens += other.rejected_prediction_tokens;
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_cached_input_token: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_cached_input_write_token: Option<f64>,
    pub per_output_token: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_image_cost: Option<ImageCostCalculationResult>,
    pub is_cache_used: bool,
}

#[derive(Serialize, Debug, PartialEq)]
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
    CompletionModelUsage(GatewayModelUsage),
    ImageGenerationModelUsage(ImageGenerationModelUsage),
}

#[async_trait::async_trait]
pub trait CostCalculator: Send + Sync {
    async fn calculate_cost(
        &self,
        model_price: &ModelPrice,
        usage: &Usage,
        credentials_ident: &CredentialsIdent,
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
    pub embedding: EmbeddingDataValue,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingDataValue {
    Float(Vec<f32>),
    Base64(Base64EmbeddingVector),
}

impl From<Vec<f32>> for EmbeddingDataValue {
    fn from(value: Vec<f32>) -> Self {
        EmbeddingDataValue::Float(value)
    }
}

impl From<Base64EmbeddingVector> for EmbeddingDataValue {
    fn from(value: Base64EmbeddingVector) -> Self {
        EmbeddingDataValue::Base64(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
    pub cost: f64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ImageModeration>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ImageModeration {
    #[default]
    Auto,
    Low,
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
            ImageSize::Other((width, height)) => write!(f, "{width}x{height}"),
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

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub struct CacheControl {
    r#type: CacheControlType,
    ttl: Option<CacheControlTtl>,
}

impl CacheControl {
    pub fn ttl(&self) -> Option<CacheControlTtl> {
        self.ttl.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CacheControlType {
    Ephemeral,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, Hash, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CacheControlTtl {
    #[default]
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "1h")]
    OneHour,
}

impl From<CacheControlTtl> for clust::messages::CacheTtl {
    fn from(val: CacheControlTtl) -> Self {
        match val {
            CacheControlTtl::FiveMinutes => clust::messages::CacheTtl::FiveMinutes,
            CacheControlTtl::OneHour => clust::messages::CacheTtl::OneHour,
        }
    }
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
                ..Default::default()
            },
            Content {
                r#type: ContentType::ImageUrl,
                image_url: Some(ImageUrl {
                    url: "https://example.com/image.jpg".to_string(),
                }),
                ..Default::default()
            },
            Content {
                r#type: ContentType::InputAudio,
                audio: Some(InputAudio {
                    data: "audio data".to_string(),
                    format: "mp3".to_string(),
                }),
                ..Default::default()
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

    #[test]
    fn test_cache_control() {
        let cache_control_initial = CacheControl {
            r#type: CacheControlType::Ephemeral,
            ttl: Some(CacheControlTtl::FiveMinutes),
        };
        let v = serde_json::to_string(&cache_control_initial).unwrap();
        println!("{v}");
        let cache_control = serde_json::from_str::<CacheControl>(&v).unwrap();
        println!("{cache_control:#?}");
        assert_eq!(cache_control_initial, cache_control);
    }

    #[test]
    fn test_message_with_cache_control() {
        let cache_control_initial = CacheControl {
            r#type: CacheControlType::Ephemeral,
            ttl: Some(CacheControlTtl::FiveMinutes),
        };
        let message = ChatCompletionMessage {
            role: "user".to_string(),
            content: Some(ChatCompletionContent::Content(vec![Content {
                r#type: ContentType::Text,
                text: Some("Hello".to_string()),
                cache_control: Some(cache_control_initial.clone()),
                ..Default::default()
            }])),
            ..Default::default()
        };
        let v = serde_json::to_string(&message).unwrap();
        println!("{v}");
        let message = serde_json::from_str::<ChatCompletionMessage>(&v).unwrap();
        println!("{message:#?}");
        assert_eq!(
            message
                .content
                .unwrap()
                .as_content()
                .unwrap()
                .first()
                .unwrap()
                .cache_control,
            Some(cache_control_initial)
        );
    }

    #[test]
    fn test_deserialize_cache_control() {
        // Test with ttl
        let json_with_ttl = r#"{"type":"ephemeral","ttl":"5m"}"#;
        let cache_control = serde_json::from_str::<CacheControl>(json_with_ttl).unwrap();
        assert_eq!(cache_control.r#type, CacheControlType::Ephemeral);
        assert_eq!(cache_control.ttl, Some(CacheControlTtl::FiveMinutes));

        // Test without ttl
        let json_without_ttl = r#"{"type":"ephemeral"}"#;
        let cache_control = serde_json::from_str::<CacheControl>(json_without_ttl).unwrap();
        assert_eq!(cache_control.r#type, CacheControlType::Ephemeral);
        assert_eq!(cache_control.ttl, None);

        // Test with one hour ttl
        let json_one_hour = r#"{"type":"ephemeral","ttl":"1h"}"#;
        let cache_control = serde_json::from_str::<CacheControl>(json_one_hour).unwrap();
        assert_eq!(cache_control.r#type, CacheControlType::Ephemeral);
        assert_eq!(cache_control.ttl, Some(CacheControlTtl::OneHour));
    }
}
