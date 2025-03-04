use std::collections::HashMap;

use crate::guards::llm_judge::LlmJudgeEvaluator;
use crate::types::Evaluator;
use crate::{
    guards::config::{load_default_guards, load_guards_from_yaml},
    types::{GuardAction, GuardDefinition, GuardStage},
};
use langdb_core::model::types::ModelEvent;
use langdb_core::model::ModelInstance;
use langdb_core::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, ChatCompletionRequest, ChatCompletionResponse,
};
use langdb_core::types::threads::Message;
use langdb_core::GatewayResult;
use serde_json::Value;

#[test]
fn test_load_guards_from_yaml() {
    let yaml = r#"
        guards:
          - type: llm_judge
            config:
              id: test-toxicity
              name: Test Toxicity
              description: Test toxicity guard
              stage: output
              action: validate
            model: gpt-3.5-turbo
            system_prompt: Test system prompt
            user_prompt_template: Test user prompt
            parameters:
              threshold: 0.5
              categories:
                - hate
                - violence
          
          - type: schema
            config:
              id: test-schema
              name: Test Schema
              description: Test schema guard
              stage: output
              action: validate
            schema:
              type: object
              properties:
                test:
                  type: string
        "#;

    let guards = load_guards_from_yaml(yaml).unwrap();
    assert_eq!(guards.len(), 2);

    // Check first guard is LlmJudge
    if let GuardDefinition::LlmJudge { config, model, .. } = &guards[0].definition {
        assert_eq!(config.definition_id, "test-toxicity");
        assert_eq!(config.definition_name, "Test Toxicity");
        assert_eq!(config.stage, GuardStage::Output);
        assert_eq!(config.action, GuardAction::Validate);
        assert_eq!(model, "gpt-3.5-turbo");
    } else {
        panic!("First guard should be LlmJudge");
    }

    // Check second guard is schema
    if let GuardDefinition::Schema { config, .. } = &guards[1].definition {
        assert_eq!(config.definition_id, "test-schema");
        assert_eq!(config.definition_name, "Test Schema");
        assert_eq!(config.stage, GuardStage::Output);
        assert_eq!(config.action, GuardAction::Validate);
    } else {
        panic!("Second guard should be Schema");
    }
}

#[tokio::test]
async fn test_guard_evaluation() {
    // Load default guards
    let guards = load_default_guards().unwrap();

    // Find the guards we need by their IDs
    let toxicity_guard = guards.get("toxicity-1").unwrap();

    let competitor_guard = guards.get("competitor-1").unwrap();

    let pii_guard = guards.get("pii-1").unwrap();

    // Test toxicity guard
    let toxic_text: TestText = "I hate you and want to kill you".into();
    let safe_text: TestText = "Hello, how are you today?".into();

    let evaluator = LlmJudgeEvaluator::new(Box::new(|_| Box::new(MockModelInstance)));

    let toxic_result = evaluator.evaluate(&toxic_text.0, &toxicity_guard);
    let safe_result = evaluator.evaluate(&safe_text.0, &toxicity_guard);

    if let crate::types::GuardResult::Boolean { passed, .. } = toxic_result.await.unwrap() {
        assert!(!passed, "Toxic text should not pass");
    }

    if let crate::types::GuardResult::Boolean { passed, .. } = safe_result.await.unwrap() {
        assert!(passed, "Safe text should pass");
    }

    // Test competitor guard
    let competitor_text: TestText = "You should try Competitor A's product".into();
    let non_competitor_text: TestText = "Our product is the best".into();

    let competitor_result = evaluator.evaluate(&competitor_text.0, &competitor_guard);
    let non_competitor_result = evaluator.evaluate(&non_competitor_text.0, &competitor_guard);

    if let crate::types::GuardResult::Text { passed, .. } = competitor_result.await.unwrap() {
        assert!(!passed, "Text with competitor should not pass");
    }

    if let crate::types::GuardResult::Boolean { passed, .. } = non_competitor_result.await.unwrap()
    {
        assert!(passed, "Text without competitor should pass");
    }

    // Test PII guard
    let pii_text: TestText = "Contact me at test@example.com or 555-123-4567".into();
    let non_pii_text: TestText = "Hello, how are you today?".into();

    let pii_result = evaluator.evaluate(&pii_text.0, &pii_guard);
    let non_pii_result = evaluator.evaluate(&non_pii_text.0, &pii_guard);

    if let crate::types::GuardResult::Text { passed, .. } = pii_result.await.unwrap() {
        assert!(!passed, "Text with PII should not pass");
    }

    if let crate::types::GuardResult::Boolean { passed, .. } = non_pii_result.await.unwrap() {
        assert!(passed, "Text without PII should pass");
    }
}

pub struct TestText(ChatCompletionRequest);

impl From<&str> for TestText {
    fn from(text: &str) -> Self {
        Self(get_request_from_text(text))
    }
}

fn get_request_from_text(text: &str) -> ChatCompletionRequest {
    ChatCompletionRequest {
        messages: vec![ChatCompletionMessage {
            role: "user".to_string(),
            content: Some(ChatCompletionContent::Text(text.to_string())),
            ..Default::default()
        }],
        ..Default::default()
    }
}

struct MockModelInstance;

#[async_trait::async_trait]
impl ModelInstance for MockModelInstance {
    async fn invoke(
        &self,
        _input_vars: HashMap<String, Value>,
        _tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        _tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessage> {
        Ok(ChatCompletionMessage {
            role: "assistant".to_string(),
            content: Some(ChatCompletionContent::Text("Hello, world!".to_string())),
            ..Default::default()
        })
    }

    async fn stream(
        &self,
        _input_vars: HashMap<String, Value>,
        _tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        _tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        todo!()
    }
}
