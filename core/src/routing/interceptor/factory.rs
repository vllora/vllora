use std::sync::Arc;

use crate::{
    executor::context::ExecutorContext,
    routing::{
        interceptor::{
            guard::RouterGuardrailInterceptor,
            rate_limiter::{RateLimiter, RateLimiterConfig},
            Interceptor, InterceptorError, InterceptorFactory, InterceptorSpec,
        },
        InterceptorType,
    },
};

pub struct RouterInterceptorFactory {
    executor_context: ExecutorContext,
}

impl RouterInterceptorFactory {
    pub fn new(executor_context: ExecutorContext) -> Self {
        Self { executor_context }
    }
}

impl InterceptorFactory for RouterInterceptorFactory {
    fn create_interceptor(
        &self,
        spec: &InterceptorSpec,
    ) -> Result<Arc<dyn Interceptor>, InterceptorError> {
        match &spec.interceptor_type {
            InterceptorType::Guardrail { guard_id } => {
                Ok(Arc::new(RouterGuardrailInterceptor::new(
                    self.executor_context.clone(),
                    spec.name.clone(),
                    guard_id.clone(),
                )))
            }
            InterceptorType::RateLimiter {
                limit,
                period,
                target,
                entity,
            } => {
                let config = RateLimiterConfig {
                    limit: *limit,
                    limit_target: target.clone(),
                    limit_entity: entity.clone(),
                    period: period.clone(),
                    burst_protection: None,
                    action: None,
                };

                let rate_limiter =
                    RateLimiter::new(self.executor_context.rate_limiter_service.clone(), config);
                Ok(Arc::new(rate_limiter))
            }
        }
    }
}
