use crate::routing::interceptor::types::{TransformDirection, TransformRule};
use crate::routing::interceptor::{Interceptor, InterceptorContext, InterceptorError};
use crate::types::gateway::{ChatCompletionContent, ChatCompletionMessage};
use regex::Regex;
use serde_json::Value;

/// Message transformer interceptor implementation
pub struct MessageTransformerInterceptor {
    rules: Vec<TransformRule>,
    direction: TransformDirection,
}

impl MessageTransformerInterceptor {
    pub fn new(rules: Vec<TransformRule>, direction: TransformDirection) -> Self {
        Self { rules, direction }
    }

    /// Apply transformation rules to a string
    fn apply_rules(&self, content: &str) -> String {
        let mut transformed = content.to_string();

        for rule in &self.rules {
            if let Ok(regex) = Regex::new(&rule.pattern) {
                let flags = rule.flags.as_deref().unwrap_or("");

                // Apply regex flags if specified
                let regex = regex;
                if flags.contains('i') {
                    // Case insensitive - handled by regex crate
                }
                if flags.contains('g') {
                    // Global replacement
                    transformed = regex
                        .replace_all(&transformed, &rule.replacement)
                        .to_string();
                } else {
                    // Single replacement
                    transformed = regex.replace(&transformed, &rule.replacement).to_string();
                }
            }
        }

        transformed
    }

    /// Transform messages in a request
    fn transform_messages(&self, messages: &mut Vec<ChatCompletionMessage>) {
        for message in messages {
            if let Some(content) = &mut message.content {
                match content {
                    ChatCompletionContent::Text(text) => {
                        *text = self.apply_rules(text);
                    }
                    ChatCompletionContent::Content(contents) => {
                        for content_item in contents {
                            if let Some(text) = &mut content_item.text {
                                *text = self.apply_rules(text);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Transform response content
    fn transform_response(&self, response: &mut Value) {
        if let Some(choices) = response.get_mut("choices") {
            if let Some(choices_array) = choices.as_array_mut() {
                for choice in choices_array {
                    if let Some(message) = choice.get_mut("message") {
                        if let Some(content) = message.get_mut("content") {
                            if let Some(content_str) = content.as_str() {
                                let transformed = self.apply_rules(content_str);
                                *content = Value::String(transformed);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl Interceptor for MessageTransformerInterceptor {
    fn name(&self) -> &str {
        "message_transformer"
    }

    async fn pre_request(
        &self,
        context: &mut InterceptorContext,
    ) -> Result<Value, InterceptorError> {
        match self.direction {
            TransformDirection::PreRequest | TransformDirection::Both => {
                self.transform_messages(&mut context.request.messages);

                Ok(serde_json::json!({
                    "transformed": true,
                    "direction": "pre_request",
                    "rules_applied": self.rules.len(),
                }))
            }
            TransformDirection::PostResponse => {
                // No transformation in pre-request for post-response direction
                Ok(serde_json::json!({
                    "transformed": false,
                    "direction": "post_response",
                    "reason": "transformation_applied_in_post_request",
                }))
            }
        }
    }

    async fn post_request(
        &self,
        _context: &mut InterceptorContext,
        response: &Value,
    ) -> Result<Value, InterceptorError> {
        match self.direction {
            TransformDirection::PostResponse | TransformDirection::Both => {
                let mut response_clone = response.clone();
                self.transform_response(&mut response_clone);

                Ok(serde_json::json!({
                    "transformed": true,
                    "direction": "post_response",
                    "rules_applied": self.rules.len(),
                    "response": response_clone,
                }))
            }
            TransformDirection::PreRequest => {
                // No transformation in post-request for pre-request direction
                Ok(serde_json::json!({
                    "transformed": false,
                    "direction": "pre_request",
                    "reason": "transformation_applied_in_pre_request",
                }))
            }
        }
    }
}

/// Factory for creating message transformer interceptors
pub struct MessageTransformerFactory;

impl MessageTransformerFactory {
    pub fn create(
        rules: Vec<TransformRule>,
        direction: TransformDirection,
    ) -> MessageTransformerInterceptor {
        MessageTransformerInterceptor::new(rules, direction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::interceptor::InterceptorState;
    use crate::types::gateway::{
        ChatCompletionContent, ChatCompletionMessage, ChatCompletionRequest,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    fn create_test_request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "openai/gpt-4".to_string(),
            messages: vec![ChatCompletionMessage {
                role: "user".to_string(),
                content: Some(ChatCompletionContent::Text("Hello, world!".to_string())),
                tool_calls: None,
                refusal: None,
                tool_call_id: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_message_transformer_basic() {
        let rules = vec![TransformRule {
            pattern: r"world".to_string(),
            replacement: "universe".to_string(),
            flags: Some("i".to_string()), // Case insensitive
        }];

        let transformer = MessageTransformerInterceptor::new(rules, TransformDirection::PreRequest);
        let mut request = create_test_request();

        // Transform the messages
        transformer.transform_messages(&mut request.messages);

        let transformed_content = request.messages[0].content.as_ref().unwrap();
        match transformed_content {
            ChatCompletionContent::Text(text) => {
                assert_eq!(text, "Hello, universe!");
            }
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_message_transformer_multiple_rules() {
        let rules = vec![
            TransformRule {
                pattern: r"world".to_string(),
                replacement: "universe".to_string(),
                flags: None,
            },
            TransformRule {
                pattern: r"Hello".to_string(),
                replacement: "Hi".to_string(),
                flags: None,
            },
        ];

        let transformer = MessageTransformerInterceptor::new(rules, TransformDirection::PreRequest);
        let mut request = create_test_request();

        transformer.transform_messages(&mut request.messages);

        let transformed_content = request.messages[0].content.as_ref().unwrap();
        match transformed_content {
            ChatCompletionContent::Text(text) => {
                assert_eq!(text, "Hi, universe!");
            }
            _ => panic!("Expected text content"),
        }
    }

    #[tokio::test]
    async fn test_message_transformer_pre_request() {
        let rules = vec![TransformRule {
            pattern: r"world".to_string(),
            replacement: "universe".to_string(),
            flags: None,
        }];

        let transformer = MessageTransformerInterceptor::new(rules, TransformDirection::PreRequest);
        let headers = HashMap::new();
        let state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
        let mut context = InterceptorContext {
            request: create_test_request(),
            headers,
            state,
            metadata: HashMap::new(),
            extra: None,
            chain_position: 0,
            results: HashMap::new(),
        };

        let result = transformer.pre_request(&mut context).await;
        assert!(result.is_ok());

        let result_value = result.unwrap();
        assert_eq!(result_value["transformed"], true);
        assert_eq!(result_value["direction"], "pre_request");

        // Check that the message was actually transformed
        let transformed_content = context.request.messages[0].content.as_ref().unwrap();
        match transformed_content {
            ChatCompletionContent::Text(text) => {
                assert_eq!(text, "Hello, universe!");
            }
            _ => panic!("Expected text content"),
        }
    }

    #[tokio::test]
    async fn test_message_transformer_post_response() {
        let rules = vec![TransformRule {
            pattern: r"world".to_string(),
            replacement: "universe".to_string(),
            flags: None,
        }];

        let transformer =
            MessageTransformerInterceptor::new(rules, TransformDirection::PostResponse);
        let headers = HashMap::new();
        let state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
        let mut context = InterceptorContext {
            request: create_test_request(),
            headers,
            state,
            metadata: HashMap::new(),
            extra: None,
            chain_position: 0,
            results: HashMap::new(),
        };

        let response = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "content": "Hello, world!"
                    }
                }
            ]
        });

        let result = transformer.post_request(&mut context, &response).await;
        assert!(result.is_ok());

        let result_value = result.unwrap();
        assert_eq!(result_value["transformed"], true);
        assert_eq!(result_value["direction"], "post_response");

        // Check that the response was transformed
        let transformed_response = result_value["response"].as_object().unwrap();
        let choices = transformed_response["choices"].as_array().unwrap();
        let message = choices[0]["message"]["content"].as_str().unwrap();
        assert_eq!(message, "Hello, universe!");
    }
}
