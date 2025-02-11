use std::borrow::Cow;
use std::{collections::HashMap, fmt::Display, ops::Deref, str::FromStr};

use crate::types::json::JsonStringCond;
use clust::messages as claude;
use indexmap::IndexMap;
use minijinja::Environment;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use serde_with::OneOrMany;
use validator::Validate;

use super::credentials::Credentials;
use super::message::MessageType;
use super::message::PromptMessage;
use super::{
    credentials::{ApiKeyCredentials, AwsCredentials},
    provider::BedrockProvider,
};
use serde::de::Error;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompletionModelDefinition {
    pub name: String,
    pub model_params: CompletionModelParams,
    pub prompt: Prompt,
    pub tools: ModelTools,
    pub db_model: Model,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub description: Option<String>,
    pub provider_name: String,
    pub prompt_name: Option<String>,
    #[serde_as(as = "JsonStringCond")]
    pub model_params: HashMap<String, Value>,
    #[serde_as(as = "JsonStringCond")]
    pub execution_options: ExecutionOptions,
    #[serde_as(as = "JsonStringCond")]
    pub tools: ModelTools,
    pub model_type: ModelType,
    pub response_schema: Option<String>,

    // Following fields are secret and virtual and not to be stored.
    // They will be borrowed from provider.
    #[serde_as(as = "Option<JsonStringCond>")]
    pub credentials: Option<Credentials>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ModelTool {
    pub name: String,
    pub description: Option<String>,
    pub passed_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(transparent)]
pub struct ModelTools(pub Vec<ModelTool>);
impl ModelTools {
    pub fn contains(&self, r: &String) -> bool {
        self.0.iter().any(|tool| &tool.name == r)
    }

    pub fn names(&self) -> impl Iterator<Item = &'_ String> {
        self.0.iter().map(|tool| &tool.name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ModelTool> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<ModelTool> for ModelTools {
    fn from_iter<T: IntoIterator<Item = ModelTool>>(iter: T) -> Self {
        Self(Vec::from_iter(iter))
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub name: String,
    #[serde_as(as = "JsonStringCond")]
    pub messages: Vec<PromptMessage>,
    pub owning_model: Option<String>,
}

impl Prompt {
    pub fn new(name: String, system_msg: String) -> Self {
        let parameters = Environment::new()
            .template_from_str(&system_msg)
            .unwrap()
            .undeclared_variables(false);

        Prompt {
            name,
            messages: vec![PromptMessage {
                r#type: MessageType::SystemMessage,
                msg: system_msg,
                parameters,
                wired: false,
            }],
            owning_model: None,
        }
    }

    pub fn get_variables(&self) -> Vec<Cow<'_, String>> {
        self.messages
            .iter()
            .flat_map(move |msg| msg.parameters.iter())
            .map(Cow::Borrowed)
            .collect()
    }

    pub fn render(template: String, variables: HashMap<String, Value>) -> String {
        let env = Environment::new();
        let tmpl = env.template_from_str(&template).unwrap();
        tmpl.render(variables).unwrap()
    }

    pub fn empty() -> Self {
        Self {
            name: "empty".to_string(),
            messages: vec![],
            owning_model: None,
        }
    }
}

impl CompletionModelDefinition {
    pub fn model_name(&self) -> String {
        match &self.model_params.engine {
            CompletionEngineParams::OpenAi { params, .. } => {
                params.model.clone().unwrap_or_default()
            }
            CompletionEngineParams::Bedrock { params, .. } => {
                params.model_id.clone().unwrap_or_default()
            }
            CompletionEngineParams::Anthropic { params, .. } => params
                .model
                .as_ref()
                .map(|m| m.to_string())
                .unwrap_or_default(),
            CompletionEngineParams::Gemini { params, .. } => {
                params.model.clone().unwrap_or_default()
            }
            CompletionEngineParams::LangdbOpen { params, .. } => {
                params.model.clone().unwrap_or_default()
            }
        }
    }

    pub fn provider_name(&self) -> String {
        match &self.model_params.engine {
            CompletionEngineParams::OpenAi { .. } => "openai".to_string(),
            CompletionEngineParams::Bedrock { provider, .. } => provider.to_string(),
            CompletionEngineParams::Anthropic { .. } => "anthropic".to_string(),
            CompletionEngineParams::Gemini { .. } => "gemini".to_string(),
            CompletionEngineParams::LangdbOpen { .. } => "langdb_open".to_string(),
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct ExecutionOptions {
    pub max_retries: Option<i32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EngineType {
    #[default]
    OpenAI,
    Bedrock,
    Anthropic,
    Gemini,
    AwsLambda,
    #[serde(rename = "langdbfunctions")]
    LangDBFunctions,
    Routing,
    Secrets,
}

impl Display for EngineType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

impl FromStr for EngineType {
    type Err = serde::de::value::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::deserialize(s.into_deserializer())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EngineFeature {
    #[default]
    Completions,
    Embeddings,
    Functions,
    Integrations,
}

impl Display for EngineFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelType {
    #[serde(rename = "completions", alias = "Completions")]
    Completions,
    #[serde(rename = "embedding", alias = "Embedding")]
    Embedding,
    #[serde(rename = "routing", alias = "Routing")]
    Routing,
    #[serde(rename = "image_generation", alias = "ImageGeneration")]
    ImageGeneration,
}

impl FromStr for ModelType {
    type Err = serde::de::value::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::deserialize(s.into_deserializer())
    }
}

impl Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::Completions => write!(f, "completions"),
            ModelType::Embedding => write!(f, "embedding"),
            ModelType::Routing => write!(f, "routing"),
            ModelType::ImageGeneration => write!(f, "image_generation"),
        }
    }
}

impl EngineType {
    pub fn supports(&self, feature: EngineFeature) -> bool {
        match (self, feature) {
            (EngineType::OpenAI, EngineFeature::Completions)
            | (EngineType::Anthropic, EngineFeature::Completions)
            | (EngineType::Gemini, EngineFeature::Completions)
            | (EngineType::Bedrock, EngineFeature::Completions)
            | (EngineType::OpenAI, EngineFeature::Embeddings)
            | (EngineType::AwsLambda, EngineFeature::Functions)
            | (EngineType::LangDBFunctions, EngineFeature::Functions) => true,

            (_, _) => false,
        }
    }

    pub fn supported_features(&self) -> &[EngineFeature] {
        match self {
            EngineType::OpenAI => &[EngineFeature::Completions, EngineFeature::Embeddings],
            EngineType::Bedrock => &[EngineFeature::Completions],
            EngineType::AwsLambda => &[EngineFeature::Functions],
            EngineType::LangDBFunctions => &[EngineFeature::Functions],
            EngineType::Anthropic => &[EngineFeature::Completions],
            EngineType::Gemini => &[EngineFeature::Completions],
            EngineType::Routing => &[EngineFeature::Completions],
            EngineType::Secrets => &[EngineFeature::Integrations],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum CompletionEngineParams {
    OpenAi {
        params: OpenAiModelParams,
        execution_options: ExecutionOptions,
        credentials: Option<ApiKeyCredentials>,
        output_schema: Option<Value>,
    },
    Bedrock {
        credentials: Option<AwsCredentials>,
        execution_options: ExecutionOptions,
        params: BedrockModelParams,
        provider: BedrockProvider,
    },
    Anthropic {
        credentials: Option<ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        params: AnthropicModelParams,
    },
    Gemini {
        credentials: Option<ApiKeyCredentials>,
        execution_options: ExecutionOptions,
        params: GeminiModelParams,
    },
    LangdbOpen {
        params: OpenAiModelParams,
        execution_options: ExecutionOptions,
        credentials: Option<ApiKeyCredentials>,
        output_schema: Option<Value>,
    },
}

impl CompletionEngineParams {
    pub fn engine_name(&self) -> &str {
        match self {
            Self::OpenAi { .. } => "openai",
            Self::Bedrock { .. } => "bedrock",
            Self::Anthropic { .. } => "anthropic",
            Self::Gemini { .. } => "gemini",
            Self::LangdbOpen { .. } => "langdb_open",
        }
    }

    pub fn provider_name(&self) -> &str {
        match self {
            Self::OpenAi { .. } => "openai",
            Self::Bedrock { provider, .. } => match provider {
                BedrockProvider::Meta => "meta",
                BedrockProvider::Mistral => "mistral",
                BedrockProvider::Cohere => "cohere",
                BedrockProvider::Other(provider) => provider.as_str(),
            },
            Self::Anthropic { .. } => "anthropic",
            Self::Gemini { .. } => "gemini",
            Self::LangdbOpen { .. } => "langdb_open",
        }
    }
}

impl CompletionEngineParams {
    pub fn model_name(&self) -> Option<&str> {
        match self {
            Self::OpenAi { params, .. } => params.model.as_deref(),
            Self::Bedrock { params, .. } => params.model_id.as_deref(),
            Self::Anthropic { params, .. } => params.model.as_ref().map(|m| m.string.as_str()),
            Self::Gemini { params, .. } => params.model.as_deref(),
            Self::LangdbOpen { params, .. } => params.model.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ImageGenerationEngineParams {
    OpenAi {
        credentials: Option<ApiKeyCredentials>,
        model_name: String,
    },
    LangdbOpen {
        credentials: Option<ApiKeyCredentials>,
        model_name: String,
    },
}

impl ImageGenerationEngineParams {
    pub fn engine_name(&self) -> String {
        match self {
            Self::OpenAi { .. } => "openai".to_string(),
            Self::LangdbOpen { .. } => "langdb_open".to_string(),
        }
    }

    pub fn provider_name(&self) -> String {
        match self {
            Self::OpenAi { .. } => "openai".to_string(),
            Self::LangdbOpen { .. } => "langdb_open".to_string(),
        }
    }
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Validate, Default)]
#[serde(deny_unknown_fields)]
pub struct OpenAiModelParams {
    /// ID of the model to use.
    /// See the [model endpoint compatibility](https://platform.openai.com/docs/models/model-endpoint-compatibility) table for details on which models work with the Chat API.
    #[serde(alias = "model_name", alias = "model_id")]
    pub model: Option<String>,

    /// Number between -2.0 and 2.0. Positive values penalize new tokens based on their existing frequency in the text so far, decreasing the model's likelihood to repeat the same line verbatim.
    ///
    #[validate(range(min = -2.0, max = 2.0))]
    pub frequency_penalty: Option<f32>, // min: -2.0, max: 2.0, default: 0

    /// Modify the likelihood of specified tokens appearing in the completion.
    ///
    /// Accepts a json object that maps tokens (specified by their token ID in the tokenizer) to an associated bias value from -100 to 100.
    /// Mathematically, the bias is added to the logits generated by the model prior to sampling.
    /// The exact effect will vary per model, but values between -1 and 1 should decrease or increase likelihood of selection;
    /// values like -100 or 100 should result in a ban or exclusive selection of the relevant token.
    pub logit_bias: Option<HashMap<String, serde_json::Value>>, // default: null

    /// Whether to return log probabilities of the output tokens or not. If true, returns the log probabilities of each output token returned in the `content` of `message`.
    pub logprobs: Option<bool>,

    /// An integer between 0 and 20 specifying the number of most likely tokens to return at each token position, each with an associated log probability. `logprobs` must be set to `true` if this parameter is used.
    #[validate(range(min = 0, max = 20))]
    pub top_logprobs: Option<u8>,

    /// The maximum number of [tokens](https://platform.openai.com/tokenizer) that can be generated in the chat completion.
    ///
    /// The total length of input tokens and generated tokens is limited by the model's context length. [Example Python code](https://cookbook.openai.com/examples/how_to_count_tokens_with_tiktoken) for counting tokens.
    pub max_tokens: Option<u32>,

    /// Number between -2.0 and 2.0. Positive values penalize new tokens based on whether they appear in the text so far, increasing the model's likelihood to talk about new topics.
    ///
    /// [See more information about frequency and presence penalties.](https://platform.openai.com/docs/api-reference/parameter-details)
    #[validate(range(min = -2.0, max = 2.0))]
    pub presence_penalty: Option<f32>, // min: -2.0, max: 2.0, default 0

    ///  This feature is in Beta.
    /// If specified, our system will make a best effort to sample deterministically, such that repeated requests
    /// with the same `seed` and parameters should return the same result.
    /// Determinism is not guaranteed, and you should refer to the `system_fingerprint` response parameter to monitor changes in the backend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,

    /// Up to 4 sequences where the API will stop generating further tokens.
    #[serde_as(as = "Option<OneOrMany<_>>")]
    #[serde(alias = "stop_sequences")]
    #[validate(length(min = 1, max = 4))]
    pub stop: Option<Vec<String>>,

    /// What sampling temperature to use, between 0 and 2. Higher values like 0.8 will make the output more random,
    /// while lower values like 0.2 will make it more focused and deterministic.
    ///
    /// We generally recommend altering this or `top_p` but not both.
    #[validate(range(min = 0.0, max = 2.0))]
    pub temperature: Option<f32>, // min: 0, max: 2, default: 1,

    /// An alternative to sampling with temperature, called nucleus sampling,
    /// where the model considers the results of the tokens with top_p probability mass.
    /// So 0.1 means only the tokens comprising the top 10% probability mass are considered.
    ///
    ///  We generally recommend altering this or `temperature` but not both.
    #[validate(range(min = 0.0, max = 1.0))]
    pub top_p: Option<f32>, // min: 0, max: 1, default: 1

    /// A unique identifier representing your end-user, which can help OpenAI to monitor and detect abuse. [Learn more](https://platform.openai.com/docs/guides/safety-best-practices/end-user-ids).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct ClaudeParams {
    anthropic_version: Option<String>,
    top_k: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct JurassicParams {
    #[validate(range(min = 0.0, max = 5.0))]
    presence_penalty: Option<f32>,
    #[validate(range(min = 0.0, max = 500.0))]
    frequency_penalty: Option<f32>,
    #[validate(range(min = 0.0, max = 1.0))]
    count_penalty: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct JambaParams {
    #[validate(range(min = 0.0, max = 5.0))]
    presence_penalty: Option<f32>,
    #[validate(range(min = 0.0, max = 500.0))]
    frequency_penalty: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct CohereCommandParams {
    #[serde(alias = "top_k")]
    #[validate(range(min = 0, max = 500))]
    k: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
enum CohereCommandRPromptTruncation {
    Off,
    AutoPreserveHistory,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct CohereCommandRParams {
    #[serde(alias = "top_k")]
    #[validate(range(min = 0, max = 500))]
    k: Option<u16>,
    preamble: Option<String>,
    prompt_truncation: Option<CohereCommandRPromptTruncation>,
    #[validate(range(min = 0.0, max = 1.0))]
    frequency_penalty: Option<f32>,
    #[validate(range(min = 0.0, max = 1.0))]
    presence_penalty: Option<f32>,
    seed: Option<u64>,
    force_single_step: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum MistralToolChoice {
    None,
    Auto,
    Any,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MistralParams {
    tool_choice: Option<MistralToolChoice>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum BedrockAdditionalModelFields {
    A21Jurassic(JurassicParams),
    A21Jamba(JambaParams),
    AmazonTitan,
    AnthropicClaude(ClaudeParams),
    CohereCommand(CohereCommandParams),
    CohereCommandR(CohereCommandRParams),
    MetaLlama,
    Mistral,
    Stability,
}

impl BedrockAdditionalModelFields {
    pub fn deserialize_with_id<'de, D: Deserializer<'de>>(
        id: &str,
        deserializer: D,
        managed_provider: Option<&BedrockProvider>,
    ) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Unit;
        let id = match managed_provider {
            Some(provider) => format!("{provider}.{id}"),
            None => id.to_string(),
        };

        Ok(if id.starts_with("a21.jamba-instruct") {
            Self::A21Jamba(JambaParams::deserialize(deserializer)?)
        } else if id.starts_with("a21.j2") {
            Self::A21Jurassic(JurassicParams::deserialize(deserializer)?)
        } else if id.starts_with("amazon.titan-text") {
            // Unit::deserialize(deserializer)?;
            Self::AmazonTitan
        } else if id.starts_with("anthropic.claude-v2") || id.starts_with("anthropic.claude-3") {
            Self::AnthropicClaude(ClaudeParams::deserialize(deserializer)?)
        } else if id.starts_with("cohere.command-text")
            || id.starts_with("cohere.command-light-text")
        {
            Self::CohereCommand(CohereCommandParams::deserialize(deserializer)?)
        } else if id.starts_with("cohere.command-r") {
            Self::CohereCommandR(CohereCommandRParams::deserialize(deserializer)?)
        } else if id.starts_with("meta.llama2") || id.starts_with("meta.llama3") {
            // Unit::deserialize(deserializer)?;
            Self::MetaLlama
        } else if id.starts_with("mistral") {
            // Unit::deserialize(deserializer)?;
            Self::Mistral
        } else {
            return Err(D::Error::custom(format!("Unknown model_id {id}")));
        })
    }
}

impl Validate for BedrockAdditionalModelFields {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        match self {
            BedrockAdditionalModelFields::A21Jurassic(p) => p.validate(),
            BedrockAdditionalModelFields::A21Jamba(p) => p.validate(),
            BedrockAdditionalModelFields::AmazonTitan => Ok(()),
            BedrockAdditionalModelFields::AnthropicClaude(p) => p.validate(),
            BedrockAdditionalModelFields::CohereCommand(p) => p.validate(),
            BedrockAdditionalModelFields::CohereCommandR(p) => p.validate(),
            BedrockAdditionalModelFields::MetaLlama => Ok(()),
            BedrockAdditionalModelFields::Mistral => Ok(()),
            BedrockAdditionalModelFields::Stability => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct BedrockModelParams {
    #[serde(alias = "model_name")]
    pub model_id: Option<String>,
    pub max_tokens: Option<i32>,
    /// The likelihood of the model selecting higher-probability options while generating a response. A lower value makes the model more likely to choose higher-probability options, while a higher value makes the model more likely to choose lower-probability options.
    /// The default value is the default value for the model that you are using. For more information, see [Inference parameters for foundation models](https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters.html)
    pub temperature: Option<f32>,
    /// The percentage of most-likely candidates that the model considers for the next token. For example, if you choose a value of 0.8 for topP, the model selects from the top 80% of the probability distribution of tokens that could be next in the sequence.
    /// The default value is the default value for the model that you are using. For more information, see [Inference parameters for foundation models](https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters.html)
    pub top_p: Option<f32>,
    /// A list of stop sequences. A stop sequence is a sequence of characters that causes the model to stop generating the response.
    #[serde(alias = "stop")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(flatten)]
    pub additional_parameters: HashMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ProviderModelParams {
    OpenAI(OpenAiModelParams),
    Bedrock(BedrockModelParams),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(from = "claude::ClaudeModel", into = "claude::ClaudeModel")]
pub struct ClaudeModel {
    model: claude::ClaudeModel,
    string: String,
}

impl Deref for ClaudeModel {
    type Target = claude::ClaudeModel;
    fn deref(&self) -> &Self::Target {
        &self.model
    }
}

impl From<claude::ClaudeModel> for ClaudeModel {
    fn from(model: claude::ClaudeModel) -> Self {
        Self {
            model,
            string: model.to_string(),
        }
    }
}

impl From<ClaudeModel> for claude::ClaudeModel {
    fn from(val: ClaudeModel) -> Self {
        val.model
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnthropicModelParams {
    /// The model that will complete your prompt.
    ///
    /// See [models](https://docs.anthropic.com/claude/docs/models-overview) for additional details and options.
    #[serde(alias = "model_name", alias = "model_id")]
    pub model: Option<ClaudeModel>,
    /// The maximum number of tokens to generate before stopping.
    ///
    /// Note that our models may stop before reaching this maximum. This parameter only specifies the absolute maximum number of tokens to generate.
    ///
    /// Different models have different maximum values for this parameter. See [models](https://docs.anthropic.com/claude/docs/models-overview) for details.
    pub max_tokens: Option<claude::MaxTokens>,
    /// Custom text sequences that will cause the model to stop generating.
    ///
    /// Our models will normally stop when they have naturally completed their turn, which will result in a response stop_reason of "end_turn".
    ///
    /// If you want the model to stop generating when it encounters custom strings of text, you can use the stop_sequences parameter. If the model encounters one of the custom sequences, the response stop_reason value will be "stop_sequence" and the response stop_sequence value will contain the matched stop sequence.
    #[serde(alias = "stop")]
    pub stop_sequences: Option<Vec<claude::StopSequence>>,
    /// Whether to incrementally stream the response using server-sent events.
    ///
    /// See [streaming](https://docs.anthropic.com/claude/reference/messages-streaming) for details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<claude::StreamOption>,
    /// Amount of randomness injected into the response.
    ///
    /// Defaults to 1.0. Ranges from 0.0 to 1.0. Use temperature closer to 0.0 for analytical / multiple choice, and closer to 1.0 for creative and generative tasks.
    ///
    /// Note that even with temperature of 0.0, the results will not be fully deterministic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<claude::Temperature>,
    /// Use nucleus sampling.
    ///
    /// In nucleus sampling, we compute the cumulative distribution over all the options for each subsequent token in decreasing probability order and cut it off once it reaches a particular probability specified by top_p. You should either alter temperature or top_p, but not both.
    ///
    /// Recommended for advanced use cases only. You usually only need to use temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<claude::TopP>,
    /// Only sample from the top K options for each subsequent token.
    ///
    /// Used to remove "long tail" low probability responses. [Learn more technical details here](https://towardsdatascience.com/how-to-sample-from-language-models-682bceb97277).
    ///
    /// Recommended for advanced use cases only. You usually only need to use temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<claude::TopK>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeminiModelParams {
    #[serde(alias = "model_name", alias = "model_id")]
    pub model: Option<String>,
    #[serde(alias = "max_tokens")]
    pub max_output_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub stop_sequences: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompletionModelParams {
    pub engine: CompletionEngineParams,
    pub provider_name: String,
    pub prompt_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum NamedArgValue {
    Value(Value),
    Identifier(String),
}

pub type NamedArgValuesMap = HashMap<String, NamedArgValue>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParentCompletionOptions {
    pub definition: Box<ParentDefinition>,
    pub named_args: NamedArgValuesMap,
    pub verbose: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct ViewParameter {
    pub name: String,
    pub r#type: ParamType,
    pub description: Option<String>,
    #[serde(default = "default_optional")]
    pub optional: bool,
}
fn default_optional() -> bool {
    false
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Default)]
pub enum ParamType {
    #[default]
    String,
    Int,
    Float,
    Boolean,
}
impl Display for ParamType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ParamType::String => "string",
            ParamType::Int => "int",
            ParamType::Float => "float",
            ParamType::Boolean => "boolean",
        })
    }
}
impl ParamType {
    pub fn from(value: String) -> Option<ParamType> {
        match value.to_lowercase().as_str() {
            "string" => Some(ParamType::String),
            "int" => Some(ParamType::Int),
            "float" => Some(ParamType::Float),
            "boolean" => Some(ParamType::Boolean),
            _ => None,
        }
    }

    pub fn sample(&self) -> Value {
        match self {
            ParamType::String => Value::String("sample".to_string()),
            ParamType::Int => Value::Number(0.into()),
            ParamType::Float => Value::Number(0.into()),
            ParamType::Boolean => Value::Bool(false),
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    pub description: Option<String>,
    pub query: String,
    #[serde_as(as = "JsonStringCond")]
    pub parameters: Vec<ViewParameter>,
    #[serde_as(as = "JsonStringCond")]
    pub schema: IndexMap<String, String>,
    pub project_id: String,
}
impl View {
    pub fn get_parameter_names(&self) -> Vec<Cow<'_, String>> {
        self.parameters
            .iter()
            .map(move |p| Cow::Borrowed(&p.name))
            .collect()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoutingModelDefinition {
    pub name: String,
    pub view: View,
    pub db_model: Model,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoutingModelOptions {
    pub definition: Box<RoutingModelDefinition>,
    pub named_args: NamedArgValuesMap,
    pub verbose: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompletionModelOptions {
    pub definition: Box<CompletionModelDefinition>,
    pub named_args: NamedArgValuesMap,
    pub verbose: bool,
}

impl From<CompletionModelOptions> for ParentCompletionOptions {
    fn from(value: CompletionModelOptions) -> Self {
        Self {
            definition: Box::new(ParentDefinition::CompletionModel(Box::new(
                value.definition.deref().clone(),
            ))),
            named_args: value.named_args,
            verbose: value.verbose,
        }
    }
}

impl From<RoutingModelOptions> for ParentCompletionOptions {
    fn from(value: RoutingModelOptions) -> Self {
        Self {
            definition: Box::new(ParentDefinition::RoutingModel(
                value.definition.deref().clone().into(),
            )),
            named_args: value.named_args,
            verbose: value.verbose,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImageGenerationModelDefinition {
    pub name: String,
    pub engine: ImageGenerationEngineParams,
    pub db_model: Model,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ParentDefinition {
    CompletionModel(Box<CompletionModelDefinition>),
    RoutingModel(Box<RoutingModelDefinition>),
    ImageGenerationModel(Box<ImageGenerationModelDefinition>),
}

impl ParentDefinition {
    pub fn get_name(&self) -> String {
        match self {
            ParentDefinition::CompletionModel(model) => model.name.clone(),
            ParentDefinition::RoutingModel(model) => model.name.clone(),
            ParentDefinition::ImageGenerationModel(image_generation_model_definition) => {
                image_generation_model_definition.name.clone()
            }
        }
    }

    pub fn get_variables(&self) -> Vec<Cow<'_, String>> {
        match self {
            ParentDefinition::CompletionModel(model) => model.prompt.get_variables(),
            ParentDefinition::RoutingModel(model) => model.view.get_parameter_names(),
            ParentDefinition::ImageGenerationModel(_) => vec![],
        }
    }
    pub fn get_db_model(&self) -> Model {
        match self {
            ParentDefinition::CompletionModel(model) => model.db_model.clone(),
            ParentDefinition::RoutingModel(model) => model.db_model.clone(),
            ParentDefinition::ImageGenerationModel(image_generation_model_definition) => {
                image_generation_model_definition.db_model.clone()
            }
        }
    }
}
