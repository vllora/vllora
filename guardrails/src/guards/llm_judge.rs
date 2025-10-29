use vllora_core::types::guardrails::GuardModel;
use vllora_core::types::guardrails::{evaluator::Evaluator, Guard, GuardResult};

use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;
use vllora_core::{
    error::GatewayError,
    llm_gateway::message_mapper::MessageMapper,
    model::ModelInstance,
    types::{
        gateway::{ChatCompletionContent, ChatCompletionMessage, ContentType},
        threads::Message,
    },
};

use super::config::{default_suffix, load_prompts_from_yaml};

#[async_trait::async_trait]
pub trait GuardModelInstanceFactory: Send + Sync {
    async fn init(&self, name: &str) -> Box<dyn ModelInstance>;
}

pub struct LlmJudgeEvaluator {
    // We'll use this to create model instances for evaluation
    pub model_factory: Box<dyn GuardModelInstanceFactory + Send + Sync>,
    pub models: HashMap<String, GuardModel>,
}

impl LlmJudgeEvaluator {
    pub fn new(model_factory: Box<dyn GuardModelInstanceFactory + Send + Sync>) -> Self {
        let models = include_str!("./config/models.yaml");
        let models = load_prompts_from_yaml(models).unwrap();
        Self {
            model_factory,
            models,
        }
    }
}

#[async_trait::async_trait]
impl Evaluator for LlmJudgeEvaluator {
    async fn evaluate(
        &self,
        messages: &[ChatCompletionMessage],
        guard: &Guard,
    ) -> Result<GuardResult, String> {
        if let Guard::LlmJudge {
            model: guard_model,
            config,
            ..
        } = &guard
        {
            // Create a model instance

            let model = match (guard_model, self.models.get(config.template_id.as_str())) {
                (Some(guard_model), _) => guard_model,
                (None, Some(model)) => model,
                _ => {
                    return Err(format!("Model not found in guard: {}", config.template_id));
                }
            };

            let model_instance = self.model_factory.init(&model.model).await;

            let input_vars: HashMap<String, Value> = match guard.parameters() {
                Some(metadata) => match serde_json::from_value(metadata.clone()) {
                    Ok(input_vars) => input_vars,
                    Err(e) => {
                        return Err(format!("Error parsing guard metadata: {e}"));
                    }
                },
                None => HashMap::new(),
            };
            // Create a channel for model events
            let (tx, _rx) = mpsc::channel(10);

            let mut guard_messages = vec![];
            if let Some(system_prompt) = &model.system_prompt {
                guard_messages.push(ChatCompletionMessage {
                    role: "system".to_string(),
                    content: Some(ChatCompletionContent::Text(system_prompt.clone())),
                    ..Default::default()
                });
            }

            let mut user_prompt_template = model.user_prompt_template.clone();

            for var in input_vars.keys() {
                user_prompt_template = user_prompt_template
                    .replace(&format!("{{{var}}}"), &input_vars[var].to_string());
            }

            user_prompt_template = format!("{}{}", user_prompt_template, default_suffix());

            if let Some(message) = messages.last() {
                let text = extract_text_content(message)?;
                user_prompt_template = user_prompt_template.replace("{{text}}", &text);
            }

            guard_messages.push(ChatCompletionMessage {
                role: "user".to_string(),
                content: Some(ChatCompletionContent::Text(user_prompt_template)),
                ..Default::default()
            });

            let guard_messages = guard_messages
                .iter()
                .map(|message| {
                    MessageMapper::map_completions_message_to_langdb_message(
                        message,
                        &model.model,
                        "judge",
                    )
                })
                .collect::<Result<Vec<Message>, GatewayError>>()
                .map_err(|e| e.to_string())?;

            // Call the model
            let result = model_instance
                .invoke(input_vars, tx, guard_messages, HashMap::new())
                .await;

            match result {
                Ok(response) => {
                    // Extract the response content
                    let content = extract_text_content(response.message())?;

                    // Try to parse as JSON
                    match serde_json::from_str::<Value>(&content) {
                        Ok(json) => {
                            let params = match &guard.parameters() {
                                Some(m) => m,
                                None => &serde_json::Value::Null,
                            };
                            // Use the parameters to determine how to interpret the response
                            Ok(interpret_json_response(json, params))
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
                Err(err) => Err(format!("LLM evaluation failed: {err}")),
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
    tracing::info!(
        "Interpreting JSON response: {:#?} and guard parameters is {:#?}",
        json,
        parameters
    );
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
                        text: format!("Found competitor mentions: {competitors}"),
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
                        text: format!("Found PII: {pii_types}"),
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
