use std::collections::HashMap;

use crate::types::gateway::FunctionParameters as FP;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CountTokensRequest {
    pub contents: Content,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CountTokensResponse {
    pub total_tokens: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GenerateContentRequest {
    pub contents: Vec<Content>,
    pub generation_config: Option<GenerationConfig>,
    pub tools: Option<Vec<Tools>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tools {
    pub function_declarations: Option<Vec<FunctionDeclaration>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Content {
    pub role: Role,
    pub parts: Vec<Part>,
}

impl From<String> for Part {
    fn from(val: String) -> Self {
        Part::Text(val)
    }
}
impl Content {
    pub fn user(part: impl Into<Part>) -> Content {
        Content {
            role: Role::User,
            parts: vec![part.into()],
        }
    }
    pub fn model(part: impl Into<Part>) -> Content {
        Content {
            role: Role::Model,
            parts: vec![part.into()],
        }
    }
}
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
    User,
    Model,
}
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    pub max_output_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub stop_sequences: Option<Vec<String>>,
    pub candidate_count: Option<u32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub seed: Option<i64>,
    pub response_logprobs: Option<bool>,
    pub logprobs: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum Part {
    Text(String),
    InlineData {
        mime_type: String,
        data: String,
    },
    FileData {
        mime_type: String,
        file_uri: String,
    },
    FunctionCall {
        name: String,
        args: HashMap<String, Value>,
    },
    FunctionResponse {
        name: String,
        response: Option<PartFunctionResponse>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PartFunctionResponse {
    pub fields: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    pub candidates: Vec<Candidate>,
    pub usage_metadata: Option<UsageMetadata>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub content: Content,
    pub citation_metadata: Option<CitationMetadata>,
    pub safety_ratings: Option<Vec<SafetyRating>>,
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SafetyRating {
    pub category: String,
    pub probability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinishReason {
    FinishReasonUnspecified, // The finish reason is unspecified.
    Stop,                    // Natural stop point of the model or provided stop sequence.
    MaxTokens,  // The maximum number of tokens as specified in the request was reached.
    Safety, // The token generation was stopped as the response was flagged for safety reasons. Note that [`Candidate`].content is empty if content filters block the output.
    Recitation, // The token generation was stopped as the response was flagged for unauthorized citations.
    Other,      // All other reasons that stopped the token
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    start_index: i32,
    end_index: i32,
    uri: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CitationMetadata {
    #[serde(default)]
    pub citations: Vec<Citation>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    pub candidates_token_count: Option<i32>,
    pub prompt_token_count: i32,
    pub total_token_count: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: FunctionParameters,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FunctionParameters {
    pub r#type: String,
    pub properties: HashMap<String, FunctionParametersProperty>,
    pub required: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FunctionParametersProperty {
    pub r#type: FunctionParametersPropertyType,
    pub description: String,
    items: Option<Box<FunctionParametersProperty>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunctionParametersPropertyType {
    Single(String),
    List(Vec<String>),
}

impl From<FP> for FunctionParameters {
    fn from(val: FP) -> FunctionParameters {
        FunctionParameters {
            r#type: val.r#type,
            properties: val
                .properties
                .iter()
                .map(|(name, p)| {
                    (
                        name.clone(),
                        FunctionParametersProperty {
                            r#type: match &p.r#type {
                                crate::types::gateway::PropertyType::Single(t) => {
                                    FunctionParametersPropertyType::Single(t.clone())
                                }
                                crate::types::gateway::PropertyType::List(t) => {
                                    FunctionParametersPropertyType::List(t.clone())
                                }
                            },
                            description: p.description.clone().unwrap_or_default(),
                            items: p.items.as_ref().map(|item| {
                                Box::new(FunctionParametersProperty::from(*item.clone()))
                            }),
                        },
                    )
                })
                .collect(),
            required: val.required,
        }
    }
}

impl From<crate::types::gateway::Property> for FunctionParametersProperty {
    fn from(val: crate::types::gateway::Property) -> Self {
        Self {
            r#type: match val.r#type {
                crate::types::gateway::PropertyType::Single(t) => {
                    FunctionParametersPropertyType::Single(t)
                }
                crate::types::gateway::PropertyType::List(t) => {
                    FunctionParametersPropertyType::List(t)
                }
            },
            description: val.description.unwrap_or_default(),
            items: val
                .items
                .map(|item| Box::new(FunctionParametersProperty::from(*item))),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelResponse {
    pub name: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub input_token_limit: Option<i64>,
    pub output_token_limit: Option<i64>,
    pub supported_generation_methods: Vec<String>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<i64>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelsResponse {
    pub models: Vec<ModelResponse>,
}
