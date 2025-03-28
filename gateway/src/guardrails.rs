use std::collections::HashMap;

use langdb_core::executor::chat_completion::resolve_model_instance;
use langdb_core::executor::context::ExecutorContext;
use langdb_core::model::ModelInstance;
use langdb_core::routing::RoutingStrategy;
use langdb_core::types::engine::ModelTools;
use langdb_core::types::gateway::ChatCompletionMessage;
use langdb_core::types::gateway::ChatCompletionRequest;
use langdb_core::types::gateway::ChatCompletionRequestWithTools;
use langdb_core::types::gateway::DynamicRouter;
use langdb_core::types::guardrails::evaluator::Evaluator;
use langdb_core::types::guardrails::service::GuardrailsEvaluator;
use langdb_core::types::guardrails::Guard;
use langdb_core::types::guardrails::GuardAction;
use langdb_core::types::guardrails::GuardResult;
use langdb_core::types::guardrails::GuardStage;
use langdb_core::types::guardrails::GuardTemplate;
use langdb_guardrails::guards::config::load_guard_templates;
use langdb_guardrails::guards::llm_judge::GuardModelInstanceFactory;
use langdb_guardrails::guards::partner::PartnerEvaluator;
use langdb_guardrails::guards::partners::openai::OpenaiGuardrailPartner;
use langdb_guardrails::guards::traced::TracedGuard;
use langdb_guardrails::guards::DatasetEvaluator;
use langdb_guardrails::guards::FileDatasetLoader;
use langdb_guardrails::guards::LlmJudgeEvaluator;
use langdb_guardrails::guards::RegexEvaluator;
use langdb_guardrails::guards::SchemaEvaluator;
use langdb_guardrails::guards::WordCountEvaluator;
use serde_json::{Map, Value};
use tracing::Span;

pub struct GuardModelFactory {
    executor_context: ExecutorContext,
}

impl GuardModelFactory {
    pub fn new(executor_context: ExecutorContext) -> Self {
        Self { executor_context }
    }
}

#[async_trait::async_trait]
impl GuardModelInstanceFactory for GuardModelFactory {
    async fn init(&self, name: &str) -> Box<dyn ModelInstance> {
        let request = ChatCompletionRequestWithTools {
            request: ChatCompletionRequest {
                model: name.to_string(),
                ..Default::default()
            },
            router: None::<DynamicRouter<RoutingStrategy>>,
            ..Default::default()
        };

        let resolved = resolve_model_instance(
            &self.executor_context,
            &request,
            HashMap::new(),
            ModelTools(vec![]),
            Span::current(),
            None,
            Vec::new(),
        )
        .await
        .expect("Failed to resolve model instance");

        resolved.model_instance
    }
}

pub struct GuardrailsService {
    guards: HashMap<String, Guard>,
    templates: HashMap<String, GuardTemplate>,
}

// Implement Send + Sync since all fields are Send + Sync
unsafe impl Send for GuardrailsService {}

impl GuardrailsService {
    pub fn new(guards: HashMap<String, Guard>) -> Self {
        let templates = load_guard_templates().unwrap_or_default();
        Self { guards, templates }
    }

    fn get_evaluator(
        &self,
        guard: &Guard,
        executor_context: &ExecutorContext,
    ) -> Result<TracedGuard, String> {
        let evaluator = match &guard {
            Guard::Schema { .. } => Box::new(SchemaEvaluator {}) as Box<dyn Evaluator>,
            Guard::LlmJudge { .. } => {
                let factory = GuardModelFactory::new(executor_context.clone());
                let evaluator = LlmJudgeEvaluator::new(
                    Box::new(factory) as Box<dyn GuardModelInstanceFactory + Send + Sync>
                );
                Box::new(evaluator) as Box<dyn Evaluator>
            }
            Guard::Dataset { .. } => Box::new(DatasetEvaluator {
                loader: Box::new(FileDatasetLoader {}),
            }) as Box<dyn Evaluator>,
            Guard::Regex { .. } => Box::new(RegexEvaluator {}) as Box<dyn Evaluator>,
            Guard::WordCount { .. } => Box::new(WordCountEvaluator {}) as Box<dyn Evaluator>,
            Guard::Partner { .. } => Box::new(PartnerEvaluator::new(Box::new(
                OpenaiGuardrailPartner::new(None).map_err(|e| e.to_string())?,
            ))) as Box<dyn Evaluator>,
        };

        Ok(TracedGuard::new(evaluator))
    }
}

#[async_trait::async_trait]
impl GuardrailsEvaluator for GuardrailsService {
    async fn evaluate(
        &self,
        messages: &[ChatCompletionMessage],
        guard_id: &str,
        executor_context: &ExecutorContext,
        parameters: Option<&serde_json::Value>,
        stage: &GuardStage,
    ) -> Result<GuardResult, String> {
        let mut guard = self
            .guards
            .get(guard_id)
            .ok_or("Guard not found".to_string())
            .cloned()?;

        if stage != guard.stage() {
            return Ok(GuardResult::Boolean {
                passed: true,
                confidence: None,
            });
        }

        let template = self
            .templates
            .get(guard.termplate_id())
            .ok_or("Guard template not found".to_string())?;

        // Extract default values from template parameters
        let default_params = template
            .parameters
            .get("properties")
            .and_then(|props| props.as_object())
            .map(|props| {
                let mut defaults = Map::new();
                for (key, value) in props {
                    if let Some(default) = value.get("default") {
                        defaults.insert(key.clone(), default.clone());
                    }
                }
                Value::Object(defaults)
            })
            .unwrap_or(Value::Object(Map::new()));

        // Start with user defined parameters from guard config
        let mut final_params = guard
            .parameters()
            .cloned()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();

        // Merge runtime parameters if provided
        if let Some(runtime_params) = parameters {
            if let Some(runtime_obj) = runtime_params.as_object() {
                for (key, value) in runtime_obj {
                    final_params.insert(key.clone(), value.clone());
                }
            }
        }

        // Finally merge with defaults for any missing values
        let empty_map = Map::new();
        let default_obj = default_params.as_object().unwrap_or(&empty_map);
        for (key, value) in default_obj {
            if !final_params.contains_key(key) {
                final_params.insert(key.clone(), value.clone());
            }
        }

        guard.set_parameters(Value::Object(final_params));

        let evaluator = self.get_evaluator(&guard, executor_context)?;
        let result = evaluator.evaluate(messages, &guard).await?;

        match guard.action() {
            GuardAction::Validate => Ok(result),
            GuardAction::Observe => Ok(GuardResult::Boolean {
                passed: true,
                confidence: None,
            }),
        }
    }
}
