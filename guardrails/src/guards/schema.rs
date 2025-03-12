use jsonschema::{Draft, JSONSchema};
use langdb_core::types::gateway::ChatCompletionMessage;
use langdb_core::types::guardrails::{evaluator::Evaluator, Guard, GuardResult};
use serde_json::Value;
use tracing::{debug, warn};

pub struct SchemaEvaluator;

#[async_trait::async_trait]
impl Evaluator for SchemaEvaluator {
    async fn evaluate(
        &self,
        messages: &[ChatCompletionMessage],
        guard: &Guard,
    ) -> Result<GuardResult, String> {
        let text = self.messages_to_text(messages)?;
        if let Guard::Schema {
            user_defined_schema,
            config,
            ..
        } = &guard
        {
            // Try to parse the text as JSON
            let json_result = serde_json::from_str::<Value>(&text);

            // Check for retry parameter in the user_defined_parameters
            let should_retry = config.user_defined_parameters
                .as_ref()
                .and_then(|params| params.get("retry"))
                .and_then(|retry| retry.as_bool())
                .unwrap_or(false);

            match json_result {
                Ok(json_value) => {
                    // Compile the schema
                    let compiled_schema = match JSONSchema::options()
                        .with_draft(Draft::Draft7)
                        .compile(user_defined_schema)
                    {
                        Ok(schema) => schema,
                        Err(e) => {
                            return Err(format!("Invalid schema definition: {}", e));
                        }
                    };

                    let json_value_clone = json_value.clone();
                    // Validate against the schema
                    let validation_result = compiled_schema.validate(&json_value_clone);
                    match validation_result {
                        Ok(_) => Ok(GuardResult::Json {
                            schema: json_value,
                            passed: true,
                        }),
                        Err(errors) => {
                            let error_messages: Vec<String> =
                                errors.map(|err| format!("{}", err)).collect();
                            let error_text = error_messages.join("; ");
                            
                            if should_retry {
                                debug!("Schema validation failed. Retry flag is set to true. Error: {}", error_text);
                                // Return a specialized result for retry
                                Ok(GuardResult::Text {
                                    text: error_text,
                                    passed: false,
                                    confidence: Some(1.0),
                                })
                            } else {
                                debug!("Schema validation failed. Retry flag is set to false.");
                                Ok(GuardResult::Text {
                                    text: error_text,
                                    passed: false,
                                    confidence: Some(1.0),
                                })
                            }
                        }
                    }
                }
                Err(e) => {
                    let error_message = format!("Invalid JSON: {}", e);
                    
                    if should_retry {
                        debug!("JSON parsing failed. Retry flag is set to true. Error: {}", error_message);
                        // Return a specialized result for retry with parsing error
                        Ok(GuardResult::Text {
                            text: error_message,
                            passed: false,
                            confidence: Some(1.0),
                        })
                    } else {
                        warn!("JSON parsing failed and retry flag is false: {}", error_message);
                        Err(error_message)
                    }
                },
            }
        } else {
            Err("Invalid guard definition".to_string())
        }
    }
}
