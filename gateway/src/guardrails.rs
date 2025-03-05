use std::collections::HashMap;

use langdb_core::executor::chat_completion::resolve_model_instance;
use langdb_core::executor::context::ExecutorContext;
use langdb_core::model::ModelInstance;
use langdb_core::routing::RoutingStrategy;
use langdb_core::types::engine::ModelTools;
use langdb_core::types::gateway::ChatCompletionRequest;
use langdb_core::types::gateway::ChatCompletionRequestWithTools;
use langdb_core::types::gateway::DynamicRouter;
use langdb_core::types::guardrails::evaluator::Evaluator;
use langdb_core::types::guardrails::service::GuardrailsEvaluator;
use langdb_core::types::guardrails::Guard;
use langdb_core::types::guardrails::GuardDefinition;
use langdb_core::types::guardrails::GuardResult;
use langdb_guardrails::guards::llm_judge::GuardModelInstanceFactory;
use langdb_guardrails::guards::traced::TracedGuard;
use langdb_guardrails::guards::DatasetEvaluator;
use langdb_guardrails::guards::FileDatasetLoader;
use langdb_guardrails::guards::LlmJudgeEvaluator;
use langdb_guardrails::guards::SchemaEvaluator;
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
        )
        .await
        .expect("Failed to resolve model instance");

        resolved.model_instance
    }
}

pub struct GuardrailsService;

// Implement Send + Sync since all fields are Send + Sync
unsafe impl Send for GuardrailsService {}
unsafe impl Sync for GuardrailsService {}

impl GuardrailsService {
    pub fn new() -> Self {
        Self {}
    }

    fn get_evaluator(
        &self,
        guard: &Guard,
        executor_context: &ExecutorContext,
    ) -> Result<TracedGuard, String> {
        let evaluator = match &guard.definition {
            GuardDefinition::Schema { .. } => Box::new(SchemaEvaluator {}) as Box<dyn Evaluator>,
            GuardDefinition::LlmJudge { .. } => {
                let factory = GuardModelFactory::new(executor_context.clone());
                let evaluator = LlmJudgeEvaluator::new(
                    Box::new(factory) as Box<dyn GuardModelInstanceFactory + Send + Sync>
                );
                Box::new(evaluator) as Box<dyn Evaluator>
            }
            GuardDefinition::Dataset { .. } => Box::new(DatasetEvaluator {
                loader: Box::new(FileDatasetLoader {}),
            }) as Box<dyn Evaluator>,
        };

        Ok(TracedGuard::new(evaluator))
    }
}

#[async_trait::async_trait]
impl GuardrailsEvaluator for GuardrailsService {
    async fn evaluate(
        &self,
        request: &ChatCompletionRequest,
        guard: &Guard,
        executor_context: &ExecutorContext,
    ) -> Result<GuardResult, String> {
        let evaluator = self.get_evaluator(guard, executor_context)?;
        evaluator.evaluate(request, guard).await
    }
}
