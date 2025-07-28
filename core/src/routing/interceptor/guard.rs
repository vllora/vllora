use crate::{
    executor::context::ExecutorContext,
    routing::interceptor::{Interceptor, InterceptorContext, InterceptorError},
    types::guardrails::{GuardResult, GuardStage},
};

pub struct RouterGuardrailInterceptor {
    executor_context: ExecutorContext,
    name: String,
    guard_id: String,
}

impl RouterGuardrailInterceptor {
    pub fn new(executor_context: ExecutorContext, name: String, guard_id: String) -> Self {
        Self {
            executor_context,
            name,
            guard_id,
        }
    }
}

#[async_trait::async_trait]
impl Interceptor for RouterGuardrailInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn pre_request(
        &self,
        context: &mut InterceptorContext,
    ) -> Result<serde_json::Value, InterceptorError> {
        tracing::warn!("Pre-request guardrail: {:#?}", self.guard_id);
        let result = self
            .executor_context
            .evaluator_service
            .evaluate(
                &context.request.messages,
                &self.guard_id,
                &self.executor_context,
                None,
                &GuardStage::Input,
            )
            .await
            .map_err(|e| InterceptorError::ExecutionError(e.to_string()))?;
        match result {
            GuardResult::Boolean { passed, confidence } => Ok(serde_json::json!({
                "passed": passed,
                "confidence": confidence
            })),
            GuardResult::Text {
                text,
                passed,
                confidence,
            } => Ok(serde_json::json!({
                "text": text,
                "passed": passed,
                "confidence": confidence
            })),
            GuardResult::Json { schema, passed } => Ok(serde_json::json!({
                "schema": schema,
                "passed": passed
            })),
        }
    }

    async fn post_request(
        &self,
        context: &mut InterceptorContext,
        response: &serde_json::Value,
    ) -> Result<serde_json::Value, InterceptorError> {
        let result = self
            .executor_context
            .evaluator_service
            .evaluate(
                &context.request.messages,
                &self.guard_id,
                &self.executor_context,
                Some(response),
                &GuardStage::Output,
            )
            .await
            .map_err(|e| InterceptorError::ExecutionError(e.to_string()))?;
        match result {
            GuardResult::Boolean { passed, confidence } => Ok(serde_json::json!({
                "passed": passed,
                "confidence": confidence
            })),
            GuardResult::Text {
                text,
                passed,
                confidence,
            } => Ok(serde_json::json!({
                "text": text,
                "passed": passed,
                "confidence": confidence
            })),
            GuardResult::Json { schema, passed } => Ok(serde_json::json!({
                "schema": schema,
                "passed": passed
            })),
        }
    }
}
