use langdb_core::types::guardrails::*;
use serde_json::json;

#[test]
fn test_schema_guard_serialization() {
    let guard = GuardDefinition::Schema {
        config: GuardConfig {
            definition_id: "schema-1".to_string(),
            definition_name: "JSON Schema Validator".to_string(),
            description: Some("Validates the response against a JSON schema".to_string()),
            stage: GuardStage::Output,
            action: GuardAction::Validate,
        },
        schema: json!({
            "type": "object",
            "required": ["name", "age"],
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer", "minimum": 0}
            }
        }),
    };

    let serialized = serde_json::to_string_pretty(&guard).unwrap();
    println!("{}", serialized);

    let deserialized: GuardDefinition = serde_json::from_str(&serialized).unwrap();
    if let GuardDefinition::Schema { config, .. } = deserialized {
        assert_eq!(config.definition_name, "JSON Schema Validator".to_string());
    } else {
        panic!("Deserialized to wrong guard type");
    }
}

#[test]
fn test_llm_judge_guard_serialization() {
    let guard = GuardDefinition::LlmJudge {
        config: GuardConfig {
            definition_id: "judge-1".to_string(),
            definition_name: "Toxicity Detector".to_string(),
            description: Some("Uses an LLM to detect toxic content".to_string()),
            stage: GuardStage::Input,
            action: GuardAction::Validate,
        },
        model: "gpt-4".to_string(),
        system_prompt: Some(
            "You are a judge evaluating whether text contains toxic content.".to_string(),
        ),
        user_prompt_template: "Evaluate if the following text contains toxic content: '{{text}}'"
            .to_string(),
        parameters: json!({
            "temperature": 0.1,
            "max_tokens": 100,
            "evaluation_criteria": [
                "Hate speech",
                "Profanity",
                "Harassment"
            ]
        }),
    };

    let serialized = serde_json::to_string_pretty(&guard).unwrap();
    println!("{}", serialized);

    let deserialized: GuardDefinition = serde_json::from_str(&serialized).unwrap();
    if let GuardDefinition::LlmJudge { model, .. } = deserialized {
        assert_eq!(model, "gpt-4".to_string());
    } else {
        panic!("Deserialized to wrong guard type");
    }
}

#[test]
fn test_dataset_guard_serialization() {
    let guard = GuardDefinition::Dataset {
        config: GuardConfig {
            definition_id: "dataset-1".to_string(),
            definition_name: "Harmful Content Detector".to_string(),
            description: Some("Uses a dataset of examples to detect harmful content".to_string()),
            stage: GuardStage::Input,
            action: GuardAction::Validate,
        },
        embedding_model: "text-embedding-ada-002".to_string(),
        threshold: 0.8,
        dataset: DatasetSource::Examples(vec![
            GuardExample {
                text: "This is harmful content...".to_string(),
                label: false,
                embedding: None,
            },
            GuardExample {
                text: "This is safe content...".to_string(),
                label: true,
                embedding: None,
            },
        ]),
        schema: json!({}),
    };

    let serialized = serde_json::to_string_pretty(&guard).unwrap();
    println!("{}", serialized);

    let deserialized: GuardDefinition = serde_json::from_str(&serialized).unwrap();
    if let GuardDefinition::Dataset {
        threshold, dataset, ..
    } = deserialized
    {
        assert_eq!(threshold, 0.8);
        if let DatasetSource::Examples(examples) = dataset {
            assert_eq!(examples.len(), 2);
        } else {
            panic!("Expected Examples dataset");
        }
    } else {
        panic!("Deserialized to wrong guard type");
    }
}

#[test]
fn test_guard_methods() {
    let schema_guard = GuardDefinition::Schema {
        config: GuardConfig {
            definition_id: "schema-1".to_string(),
            definition_name: "JSON Schema Validator".to_string(),
            description: Some("Validates the response against a JSON schema".to_string()),
            stage: GuardStage::Output,
            action: GuardAction::Validate,
        },
        schema: json!({}),
    };

    assert_eq!(schema_guard.stage(), &GuardStage::Output);
    assert_eq!(schema_guard.action(), &GuardAction::Validate);
    assert_eq!(schema_guard.name(), &"JSON Schema Validator".to_string());
}

#[test]
fn test_guard_deserialization_from_json() {
    // Schema guard deserialization
    let schema_json = r#"{
        "type": "schema",
        "id": "schema-1",
        "name": "JSON Schema Validator",
        "description": "Validates the response against a JSON schema",
        "stage": "output",
        "action": "validate",
        "schema": {
            "type": "object",
            "required": ["name", "age"],
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer", "minimum": 0}
            }
        }
    }"#;

    let guard: GuardDefinition = serde_json::from_str(schema_json).unwrap();
    if let GuardDefinition::Schema { config, .. } = guard {
        assert_eq!(config.definition_id, "schema-1".to_string());
        assert_eq!(config.stage, GuardStage::Output);
    } else {
        panic!("Failed to deserialize schema guard");
    }

    // LLM Judge guard deserialization
    let llm_judge_json = r#"{
        "type": "llmjudge",
        "id": "judge-1",
        "name": "Toxicity Detector",
        "description": "Uses an LLM to detect toxic content",
        "stage": "input",
        "action": "validate",
        "model": "gpt-4",
        "systemPrompt": "You are a judge evaluating whether text contains toxic content.",
        "userPromptTemplate": "Evaluate if the following text contains toxic content: '{{text}}'",
        "parameters": {
            "temperature": 0.1,
            "max_tokens": 100,
            "evaluation_criteria": ["Hate speech", "Profanity", "Harassment"]
        }
    }"#;

    let guard: GuardDefinition = serde_json::from_str(llm_judge_json).unwrap();
    if let GuardDefinition::LlmJudge {
        config,
        model,
        parameters,
        ..
    } = guard
    {
        assert_eq!(config.definition_id, "judge-1".to_string());
        assert_eq!(model, "gpt-4");

        // Extract evaluation_criteria from parameters
        if let Some(criteria) = parameters.get("evaluation_criteria") {
            if let Some(criteria_array) = criteria.as_array() {
                assert_eq!(criteria_array.len(), 3);
                assert_eq!(criteria_array[0], "Hate speech");
            } else {
                panic!("evaluation_criteria is not an array");
            }
        } else {
            panic!("No evaluation_criteria in parameters");
        }
    } else {
        panic!("Failed to deserialize LLM judge guard");
    }

    // Dataset guard deserialization
    let dataset_json = r#"{
        "type": "dataset",
        "id": "dataset-1",
        "name": "Harmful Content Detector",
        "description": "Uses a dataset of examples to detect harmful content",
        "stage": "input",
        "action": "validate",
        "embedding_model": "text-embedding-ada-002",
        "threshold": 0.8,
        "dataset": {
            "type": "source",
            "value": "harmful-content-dataset"
        }
    }"#;

    let guard: GuardDefinition = serde_json::from_str(dataset_json).unwrap();
    if let GuardDefinition::Dataset {
        config,
        embedding_model,
        dataset,
        ..
    } = guard
    {
        assert_eq!(config.definition_id, "dataset-1".to_string());
        assert_eq!(embedding_model, "text-embedding-ada-002");
        if let DatasetSource::Source(source) = dataset {
            assert_eq!(source, "harmful-content-dataset");
        } else {
            panic!("Expected Source dataset");
        }
    } else {
        panic!("Failed to deserialize dataset guard");
    }
}

#[test]
fn test_guard_result_deserialization() {
    // Boolean result
    let boolean_result_json = r#"{
        "type": "boolean",
        "passed": true,
        "confidence": 0.95
    }"#;

    let result: GuardResult = serde_json::from_str(boolean_result_json).unwrap();
    if let GuardResult::Boolean { passed, confidence } = result {
        assert!(passed);
        assert_eq!(confidence, Some(0.95));
    } else {
        panic!("Failed to deserialize boolean result");
    }

    // Text result
    let text_result_json = r#"{
        "type": "text",
        "text": "Content contains harmful language",
        "passed": false,
        "confidence": 0.87
    }"#;

    let result: GuardResult = serde_json::from_str(text_result_json).unwrap();
    if let GuardResult::Text {
        text,
        passed,
        confidence,
    } = result
    {
        assert_eq!(text, "Content contains harmful language");
        assert!(!passed);
        assert_eq!(confidence, Some(0.87));
    } else {
        panic!("Failed to deserialize text result");
    }

    // JSON result
    let json_result_json = r#"{
        "type": "json",
        "schema": {"validation": "failed", "errors": ["Missing required field"]},
        "passed": false
    }"#;

    let result: GuardResult = serde_json::from_str(json_result_json).unwrap();
    if let GuardResult::Json { schema, passed } = result {
        assert!(!passed);
        assert_eq!(schema["validation"], "failed");
    } else {
        panic!("Failed to deserialize JSON result");
    }
}
