use std::collections::HashMap;

use crate::guards::config::load_guards_from_yaml;
use crate::guards::llm_judge::LlmJudgeEvaluator;
use serde_json::Value;
use vllora_core::model::types::ModelEvent;
use vllora_core::model::types::ModelFinishReason;
use vllora_core::model::ModelInstance;
use vllora_core::types::gateway::ChatCompletionMessageWithFinishReason;
use vllora_core::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, ChatCompletionRequest,
};
use vllora_core::types::guardrails::evaluator::Evaluator;
use vllora_core::types::guardrails::{Guard, GuardAction, GuardStage};
use vllora_core::types::threads::Message;
use vllora_core::GatewayResult;

use super::llm_judge::GuardModelInstanceFactory;

fn default_test_guards() -> Result<HashMap<String, Guard>, serde_yaml::Error> {
    let yaml = r#"
        guards:
            toxicity-1:
                type: llm_judge
                id: toxicity-1
                name: Toxicity Detection
                template_id: content-toxicity
                description: Detects toxic, harmful, or inappropriate content
                stage: output
                action: validate
                user_defined_parameters:
                    threshold: 0.5
                    categories:
                        - hate
                        - violence

            competitor-1:
                type: llm_judge
                id: competitor-1
                name: Toxicity Detection
                template_id: content-competitor-mentions
                description: Detects toxic, harmful, or inappropriate content
                stage: output
                action: validate
                user_defined_parameters:
                    threshold: 0.5
                    competitors:
                        - Competitor A
                        - Competitor B

            pii-1:
                type: llm_judge
                id: pii-1
                name: Toxicity Detection
                template_id: security-pii-detection
                description: Detects toxic, harmful, or inappropriate content
                stage: output
                action: validate
                user_defined_parameters:
                    threshold: 0.5
                    pii_types:
                        - email
                        - phone
                        - name
            schema-1:
                type: schema
                id: schema-1
                template_id: validation-json-schema
                name: Test Schema
                description: Test schema guard
                stage: output
                action: validate
                user_defined_schema:
                    type: object
                    properties:
                        - name: test
                          type: string
        "#;

    load_guards_from_yaml(yaml)
}

#[test]
fn test_load_guards_from_yaml() {
    let guards = default_test_guards().unwrap();
    assert_eq!(guards.len(), 4);
    // Check first guard is LlmJudge
    if let Guard::LlmJudge { config, .. } = &guards["toxicity-1"] {
        assert_eq!(config.id, "toxicity-1");
        assert_eq!(config.name, "Toxicity Detection");
        assert_eq!(config.stage, GuardStage::Output);
        assert_eq!(config.action, GuardAction::Validate);
    } else {
        panic!("First guard should be LlmJudge");
    }

    // Check second guard is schema
    if let Guard::Schema { config, .. } = &guards["schema-1"] {
        assert_eq!(config.id, "schema-1");
        assert_eq!(config.name, "Test Schema");
        assert_eq!(config.stage, GuardStage::Output);
        assert_eq!(config.action, GuardAction::Validate);
    } else {
        panic!("Second guard should be Schema");
    }
}

#[tokio::test]
async fn test_guard_evaluation() {
    // Load default guards
    let guards = default_test_guards().unwrap();

    // Find the guards we need by their IDs
    let toxicity_guard = guards.get("toxicity-1").unwrap();

    let competitor_guard = guards.get("competitor-1").unwrap();

    let pii_guard = guards.get("pii-1").unwrap();

    // Test toxicity guard
    let toxic_text: TestText = "I hate you and want to kill you".into();
    let safe_text: TestText = "Hello, how are you today?".into();

    let safe_evaluator = LlmJudgeEvaluator::new(Box::new(MockGuardModelInstanceFactory(
        "{\"passed\":true}".to_string(),
    )));
    let toxic_evaluator = LlmJudgeEvaluator::new(Box::new(MockGuardModelInstanceFactory(
        "{\"passed\":false}".to_string(),
    )));

    let toxic_result = toxic_evaluator.evaluate(&toxic_text.0.messages, toxicity_guard);
    let safe_result = safe_evaluator.evaluate(&safe_text.0.messages, toxicity_guard);

    if let vllora_core::types::guardrails::GuardResult::Boolean { passed, .. } =
        toxic_result.await.unwrap()
    {
        assert!(!passed, "Toxic text should not pass");
    }

    if let vllora_core::types::guardrails::GuardResult::Boolean { passed, .. } =
        safe_result.await.unwrap()
    {
        assert!(passed, "Safe text should pass");
    }

    let competitor_evaluator = LlmJudgeEvaluator::new(Box::new(MockGuardModelInstanceFactory(
        "{\"mentions_competitor\":true}".to_string(),
    )));
    let non_competitor_evaluator = LlmJudgeEvaluator::new(Box::new(MockGuardModelInstanceFactory(
        "{\"mentions_competitor\":false}".to_string(),
    )));

    // Test competitor guard
    let competitor_text: TestText = "You should try Competitor A's product".into();
    let non_competitor_text: TestText = "Our product is the best".into();

    let competitor_result =
        competitor_evaluator.evaluate(&competitor_text.0.messages, competitor_guard);
    let non_competitor_result =
        non_competitor_evaluator.evaluate(&non_competitor_text.0.messages, competitor_guard);

    if let vllora_core::types::guardrails::GuardResult::Text { passed, .. } =
        competitor_result.await.unwrap()
    {
        assert!(!passed, "Text with competitor should not pass");
    }

    if let vllora_core::types::guardrails::GuardResult::Boolean { passed, .. } =
        non_competitor_result.await.unwrap()
    {
        assert!(passed, "Text without competitor should pass");
    }

    let pii_evaluator = LlmJudgeEvaluator::new(Box::new(MockGuardModelInstanceFactory(
        "{\"contains_pii\":true}".to_string(),
    )));
    let non_pii_evaluator = LlmJudgeEvaluator::new(Box::new(MockGuardModelInstanceFactory(
        "{\"contains_pii\":false}".to_string(),
    )));

    // Test PII guard
    let pii_text: TestText = "Contact me at test@example.com or 555-123-4567".into();
    let non_pii_text: TestText = "Hello, how are you today?".into();

    let pii_result = pii_evaluator.evaluate(&pii_text.0.messages, pii_guard);
    let non_pii_result = non_pii_evaluator.evaluate(&non_pii_text.0.messages, pii_guard);

    if let vllora_core::types::guardrails::GuardResult::Text { passed, .. } =
        pii_result.await.unwrap()
    {
        assert!(!passed, "Text with PII should not pass");
    }

    if let vllora_core::types::guardrails::GuardResult::Boolean { passed, .. } =
        non_pii_result.await.unwrap()
    {
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

struct MockModelInstance(String);

#[async_trait::async_trait]
impl ModelInstance for MockModelInstance {
    async fn invoke(
        &self,
        _input_vars: HashMap<String, Value>,
        _tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        _previous_messages: Vec<Message>,
        _tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessageWithFinishReason> {
        Ok(ChatCompletionMessageWithFinishReason::new(
            ChatCompletionMessage {
                role: "assistant".to_string(),
                content: Some(ChatCompletionContent::Text(self.0.clone())),
                ..Default::default()
            },
            ModelFinishReason::Stop,
        ))
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

struct MockGuardModelInstanceFactory(String);

#[async_trait::async_trait]
impl GuardModelInstanceFactory for MockGuardModelInstanceFactory {
    async fn init(&self, _name: &str) -> Box<dyn ModelInstance> {
        Box::new(MockModelInstance(self.0.clone()))
    }
}
