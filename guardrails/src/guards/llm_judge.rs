use langdb_core::types::guardrails::{Evaluator, Guard, GuardDefinition, GuardResult};

use langdb_core::{
    error::GatewayError,
    llm_gateway::message_mapper::MessageMapper,
    model::ModelInstance,
    types::{
        gateway::{
            ChatCompletionContent, ChatCompletionMessage, ChatCompletionRequest, ContentType,
        },
        threads::Message,
    },
};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;

pub struct LlmJudgeEvaluator {
    // We'll use this to create model instances for evaluation
    pub model_factory: Box<dyn Fn(&str) -> Box<dyn ModelInstance> + Send + Sync>,
}

impl LlmJudgeEvaluator {
    pub fn new(model_factory: Box<dyn Fn(&str) -> Box<dyn ModelInstance> + Send + Sync>) -> Self {
        Self { model_factory }
    }
}

#[async_trait::async_trait]
impl Evaluator for LlmJudgeEvaluator {
    async fn evaluate(
        &self,
        request: &ChatCompletionRequest,
        guard: &Guard,
    ) -> Result<GuardResult, String> {
        if let GuardDefinition::LlmJudge {
            model,
            system_prompt,

            parameters,
            ..
        } = &guard.definition
        {
            // Create a model instance
            let model_instance = (self.model_factory)(model);

            let input_vars = match serde_json::from_value(guard.metadata.clone()) {
                Ok(input_vars) => input_vars,
                Err(e) => {
                    return Err(format!("Error parsing guard metadata: {}", e));
                }
            };

            // Create a channel for model events
            let (tx, _rx) = mpsc::channel(10);

            let mut messages = vec![];
            if let Some(system_prompt) = system_prompt {
                messages.push(ChatCompletionMessage {
                    role: "system".to_string(),
                    content: Some(ChatCompletionContent::Text(system_prompt.clone())),
                    ..Default::default()
                });
            }
            if let Some(msg) = request.messages.last() {
                messages.push(msg.clone());
            }

            let messages = messages
                .iter()
                .map(|message| {
                    MessageMapper::map_completions_message_to_langdb_message(
                        message,
                        &request.model,
                        "judge",
                    )
                })
                .collect::<Result<Vec<Message>, GatewayError>>()
                .map_err(|e| e.to_string())?;

            // Call the model
            let result = model_instance
                .invoke(input_vars, tx, messages, HashMap::new())
                .await;

            match result {
                Ok(response) => {
                    // Extract the response content
                    let content = extract_text_content(&response)?;

                    // Try to parse as JSON
                    match serde_json::from_str::<Value>(&content) {
                        Ok(json) => {
                            // Use the parameters to determine how to interpret the response
                            Ok(interpret_json_response(json, parameters))
                        }
                        Err(_) => {
                            // If it's not JSON, just return the text
                            Ok(GuardResult::Text {
                                text: content,
                                passed: true,
                                confidence: None,
                            })
                        }
                    }
                }
                Err(err) => Err(format!("LLM evaluation failed: {}", err)),
            }
        } else {
            Err("Guard definition is not a LlmJudge".to_string())
        }
    }
}

// Extract text content from a ChatCompletionMessage
fn extract_text_content(response: &ChatCompletionMessage) -> Result<String, String> {
    match &response.content {
        Some(ChatCompletionContent::Text(text)) => Ok(text.clone()),
        Some(ChatCompletionContent::Content(arr)) => {
            // Find the first text content
            let text_content = arr.iter().find_map(|content| {
                if let ContentType::Text = content.r#type {
                    content.text.clone()
                } else {
                    None
                }
            });

            match text_content {
                Some(text) => Ok(text),
                None => Err("No text content found in response".to_string()),
            }
        }
        None => Err("No content found in response".to_string()),
    }
}

// Interpret JSON response based on parameters
fn interpret_json_response(json: Value, parameters: &Value) -> GuardResult {
    // Check for common result fields first
    if let Some(passed) = json.get("passed").and_then(|v| v.as_bool()) {
        let confidence = json.get("confidence").and_then(|v| v.as_f64());
        let details = json
            .get("details")
            .and_then(|v| v.as_str())
            .map(String::from);

        return if let Some(details) = details {
            GuardResult::Text {
                text: details,
                passed,
                confidence,
            }
        } else {
            GuardResult::Boolean { passed, confidence }
        };
    }

    // Look for guard-specific fields based on parameters
    if parameters.get("threshold").is_some() {
        // Toxicity guard
        if let Some(toxic) = json.get("toxic").and_then(|v| v.as_bool()) {
            let confidence = json.get("confidence").and_then(|v| v.as_f64());
            return GuardResult::Boolean {
                passed: !toxic,
                confidence,
            };
        }
    }

    if parameters.get("competitors").is_some() {
        // Competitor guard
        if let Some(mentions) = json.get("mentions_competitor").and_then(|v| v.as_bool()) {
            let confidence = Some(if mentions { 0.9 } else { 0.1 });

            if mentions {
                if let Some(found) = json.get("competitors_found").and_then(|v| v.as_array()) {
                    let competitors = found
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");

                    return GuardResult::Text {
                        text: format!("Found competitor mentions: {}", competitors),
                        passed: false,
                        confidence,
                    };
                }
            }

            return GuardResult::Boolean {
                passed: !mentions,
                confidence,
            };
        }
    }

    if parameters.get("pii_types").is_some() {
        // PII guard
        if let Some(contains_pii) = json.get("contains_pii").and_then(|v| v.as_bool()) {
            let confidence = Some(if contains_pii { 0.9 } else { 0.1 });

            if contains_pii {
                if let Some(types) = json.get("pii_types").and_then(|v| v.as_array()) {
                    let pii_types = types
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");

                    return GuardResult::Text {
                        text: format!("Found PII: {}", pii_types),
                        passed: false,
                        confidence,
                    };
                }
            }

            return GuardResult::Boolean {
                passed: !contains_pii,
                confidence,
            };
        }
    }

    // If we can't determine the result format, return the JSON as text
    GuardResult::Text {
        text: json.to_string(),
        passed: true,
        confidence: None,
    }
}
