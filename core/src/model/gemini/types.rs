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
    #[serde(default)]
    pub parts: Vec<PartWithThought>,
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
            parts: vec![PartWithThought {
                part: part.into(),
                thought_signature: None,
            }],
        }
    }
    pub fn model(part: impl Into<Part>) -> Content {
        Content {
            role: Role::Model,
            parts: vec![PartWithThought {
                part: part.into(),
                thought_signature: None,
            }],
        }
    }

    pub fn user_with_multiple_parts(parts: Vec<PartWithThought>) -> Content {
        Content {
            role: Role::User,
            parts,
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
    pub response_mime_type: Option<String>,
    pub response_schema: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PartWithThought {
    #[serde(flatten)]
    pub part: Part,
    pub thought_signature: Option<String>,
}

impl From<Part> for PartWithThought {
    fn from(part: Part) -> Self {
        Self {
            part,
            thought_signature: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PartFunctionResponse {
    pub fields: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    #[serde(default)]
    pub candidates: Vec<Candidate>,
    pub usage_metadata: Option<UsageMetadata>,
    pub model_version: String,
    pub response_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinishReason {
    FinishReasonUnspecified, // Default value. This value is unused.
    Stop,                    // Natural stop point of the model or provided stop sequence.
    MaxTokens,  // The maximum number of tokens as specified in the request was reached.
    Safety,     // The response candidate content was flagged for safety reasons.
    Recitation, // The response candidate content was flagged for recitation reasons.
    Language,   // The response candidate content was flagged for using an unsupported language.
    Other,      // Unknown reason.
    Blocklist,  // Token generation stopped because the content contains forbidden terms.
    ProhibitedContent, // Token generation stopped for potentially containing prohibited content.
    Spii, // Token generation stopped because the content potentially contains Sensitive Personally Identifiable Information (SPII).
    MalformedFunctionCall, // The function call generated by the model is invalid.
    ImageSafety, // Token generation stopped because generated images contain safety violations.
    UnexpectedToolCall, // Model generated a tool call but no tools were enabled in the request.
    TooManyToolCalls, // Token generation stopped because too many tool calls were generated.
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
    pub thoughts_token_count: Option<i32>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<FunctionParametersPropertyType>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<Box<FunctionParametersProperty>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
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
                            r#type: p.r#type.as_ref().map(|t| match t {
                                crate::types::gateway::PropertyType::Single(t) => {
                                    FunctionParametersPropertyType::Single(t.clone())
                                }
                                crate::types::gateway::PropertyType::List(t) => {
                                    FunctionParametersPropertyType::List(t.clone())
                                }
                            }),
                            description: p.description.clone().unwrap_or_default(),
                            items: p.items.as_ref().map(|item| {
                                Box::new(FunctionParametersProperty::from(*item.clone()))
                            }),
                        },
                    )
                })
                .collect(),
            required: val.required.unwrap_or_default(),
        }
    }
}

impl From<crate::types::gateway::Property> for FunctionParametersProperty {
    fn from(val: crate::types::gateway::Property) -> Self {
        Self {
            r#type: val.r#type.as_ref().map(|t| match t {
                crate::types::gateway::PropertyType::Single(t) => {
                    FunctionParametersPropertyType::Single(t.clone())
                }
                crate::types::gateway::PropertyType::List(t) => {
                    FunctionParametersPropertyType::List(t.clone())
                }
            }),
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateEmbeddingRequest {
    pub content: ContentPart,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<TaskType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dimensionality: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContentPart {
    pub parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskType {
    TaskTypeUnspecified,
    RetrievalQuery,
    RetrievalDocument,
    SemanticSimilarity,
    Classification,
    Clustering,
    QuestionAnswering,
    FactVerification,
    CodeRetrievalQuery,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmbeddingsValue {
    pub values: Vec<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateEmbeddingResponse {
    pub embedding: EmbeddingsValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_content_request() {
        let response = r#"
        {
            "candidates": [
                {
                "content": {
                    "role": "model"
                },
                "finishReason": "MAX_TOKENS",
                "index": 0
                }
            ],
            "usageMetadata": {
                "promptTokenCount": 4,
                "totalTokenCount": 13,
                "promptTokensDetails": [
                {
                    "modality": "TEXT",
                    "tokenCount": 4
                }
                ],
                "thoughtsTokenCount": 9
            },
            "modelVersion": "gemini-2.5-flash",
            "responseId": "0PLTaLCgI6Ko_uMP-ane4A8"
            }
        "#;

        let response = serde_json::from_str::<GenerateContentResponse>(response).unwrap();

        assert_eq!(response.candidates.len(), 1);
        assert_eq!(response.candidates[0].content.parts.len(), 0);
        assert_eq!(
            response.candidates[0]
                .finish_reason
                .as_ref()
                .unwrap()
                .clone(),
            FinishReason::MaxTokens
        );
    }
}
