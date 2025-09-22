use crate::routing::interceptor::{InterceptorFactory, LazyInterceptorManager};
use crate::routing::{
    strategy::conditional::evaluator::{evaluate_conditions, referenced_pre_request_interceptors},
    ConditionalRouting, TargetSpec,
};

pub struct ConditionalRouter {
    pub routing: ConditionalRouting,
}

impl ConditionalRouter {
    /// Evaluates routes in order, running only referenced pre_request interceptors lazily, and returns the first matching target.
    /// Stops at the first unmet condition for each route and moves to the next route.
    /// Accepts an InterceptorFactory to instantiate interceptors as needed.
    pub async fn get_target(
        &self,
        factory: Box<dyn InterceptorFactory>,
        request: &crate::types::gateway::ChatCompletionRequest,
        headers: &std::collections::HashMap<String, String>,
        metadata: &std::collections::HashMap<String, serde_json::Value>,
        extra: Option<&crate::types::gateway::Extra>,
    ) -> Option<&TargetSpec> {
        let referenced = referenced_pre_request_interceptors(&self.routing.routes);

        // Create interceptors map for lazy execution
        let mut interceptors = std::collections::HashMap::new();
        for spec in &self.routing.pre_request {
            if referenced.contains(&spec.name) {
                if let Ok(interceptor) = factory.create_interceptor(spec) {
                    interceptors.insert(spec.name.clone(), interceptor);
                }
            }
        }

        let state = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::routing::interceptor::InterceptorState::new(),
        ));
        let context = crate::routing::interceptor::InterceptorContext::new(
            request.clone(),
            extra.cloned(),
            headers.clone(),
            state.clone(),
        )
        .with_extra(extra.cloned());

        // Create lazy interceptor manager
        let mut lazy_manager = LazyInterceptorManager::new(interceptors, context);

        // Evaluate routes in order with lazy interceptor execution
        for route in &self.routing.routes {
            if let Some(conditions) = &route.conditions {
                match evaluate_conditions(conditions, &mut lazy_manager, metadata, extra).await {
                    Ok(true) => {
                        let span = tracing::Span::current();
                        span.record("router.execution_route", &route.name);
                        if let Some(targets) = &route.targets {
                            return Some(targets);
                        }
                    }
                    Ok(false) => {
                        // Condition not met, continue to next route
                        continue;
                    }
                    Err(e) => {
                        tracing::error!(
                            "Error evaluating conditions for route {}: {}",
                            route.name,
                            e
                        );
                        continue;
                    }
                }
            } else if let Some(targets) = &route.targets {
                return Some(targets);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::interceptor;
    use crate::routing::interceptor::{Interceptor, InterceptorContext, InterceptorError};
    use crate::routing::ConditionOpType;
    use crate::routing::{
        ConditionOp, ConditionalRouting, InterceptorSpec, InterceptorType, Route, RouteCondition,
        TargetSpec,
    };
    use crate::types::gateway::ChatCompletionRequest;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MockGuardrail {
        result: bool,
    }
    #[async_trait::async_trait]
    impl Interceptor for MockGuardrail {
        fn name(&self) -> &str {
            "guardrail"
        }
        async fn pre_request(
            &self,
            _context: &mut InterceptorContext,
        ) -> Result<serde_json::Value, InterceptorError> {
            Ok(serde_json::json!({"result": self.result}))
        }
        async fn post_request(
            &self,
            _context: &mut InterceptorContext,
            _response: &serde_json::Value,
        ) -> Result<serde_json::Value, InterceptorError> {
            Ok(serde_json::json!({"result": self.result}))
        }
    }

    struct MockFactory {
        result: bool,
    }

    impl interceptor::InterceptorFactory for MockFactory {
        fn create_interceptor(
            &self,
            spec: &InterceptorSpec,
        ) -> Result<Arc<dyn Interceptor>, InterceptorError> {
            // Handle both "guardrail" and "guardrail1", "guardrail2" patterns
            if spec.name.starts_with("guardrail") {
                Ok(Arc::new(MockGuardrail {
                    result: self.result,
                }))
            } else {
                Err(InterceptorError::ExecutionError(
                    "Unknown interceptor".to_string(),
                ))
            }
        }
    }

    #[tokio::test]
    async fn test_guardrail_passes() {
        let routing = ConditionalRouting {
            pre_request: vec![InterceptorSpec {
                name: "guardrail".to_string(),
                interceptor_type: InterceptorType::Guardrail {
                    guard_id: "guard_id".to_string(),
                },
                extra: HashMap::new(),
            }],
            routes: vec![Route {
                name: "guarded_route".to_string(),
                conditions: Some(RouteCondition::Expr(HashMap::from([(
                    "pre_request.guardrail.result".to_string(),
                    ConditionOp {
                        op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
                    },
                )]))),
                targets: Some(TargetSpec::List(vec![HashMap::from([(
                    "model".to_string(),
                    serde_json::json!("mock/model"),
                )])])),
                message_mapper: None,
            }],
            post_request: vec![],
        };
        let router = ConditionalRouter { routing };
        let factory = Box::new(MockFactory { result: true }) as Box<dyn InterceptorFactory>;
        let target = router
            .get_target(
                factory,
                &ChatCompletionRequest::default(),
                &HashMap::new(),
                &HashMap::new(),
                None,
            )
            .await;
        assert!(target.is_some());
        if let Some(TargetSpec::List(targets)) = target {
            assert_eq!(targets[0]["model"], "mock/model");
        } else {
            panic!("Expected List target");
        }
    }

    #[tokio::test]
    async fn test_guardrail_fails() {
        let routing = ConditionalRouting {
            pre_request: vec![InterceptorSpec {
                name: "guardrail".to_string(),
                interceptor_type: InterceptorType::Guardrail {
                    guard_id: "guard_id".to_string(),
                },
                extra: HashMap::new(),
            }],
            routes: vec![Route {
                name: "guarded_route".to_string(),
                conditions: Some(RouteCondition::Expr(HashMap::from([(
                    "pre_request.guardrail.result".to_string(),
                    ConditionOp {
                        op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
                    },
                )]))),
                targets: Some(TargetSpec::List(vec![HashMap::from([(
                    "model".to_string(),
                    serde_json::json!("mock/model"),
                )])])),
                message_mapper: None,
            }],
            post_request: vec![],
        };
        let router = ConditionalRouter { routing };
        let factory = Box::new(MockFactory { result: false }) as Box<dyn InterceptorFactory>;
        let target = router
            .get_target(
                factory,
                &ChatCompletionRequest::default(),
                &HashMap::new(),
                &HashMap::new(),
                None,
            )
            .await;
        assert!(target.is_none());
    }

    #[tokio::test]
    async fn test_no_referenced_interceptors_metadata_only() {
        let routing = ConditionalRouting {
            pre_request: vec![],
            routes: vec![Route {
                name: "meta_route".to_string(),
                conditions: Some(RouteCondition::Expr(HashMap::from([(
                    "metadata.region".to_string(),
                    ConditionOp {
                        op: HashMap::from([(ConditionOpType::Eq, serde_json::json!("Europe"))]),
                    },
                )]))),
                targets: Some(TargetSpec::List(vec![HashMap::from([(
                    "model".to_string(),
                    serde_json::json!("meta/model"),
                )])])),
                message_mapper: None,
            }],
            post_request: vec![],
        };
        let router = ConditionalRouter { routing };
        let factory = Box::new(MockFactory { result: true }) as Box<dyn InterceptorFactory>; // result doesn't matter
        let mut metadata = HashMap::new();
        metadata.insert("region".to_string(), serde_json::json!("Europe"));
        let target = router
            .get_target(
                factory,
                &ChatCompletionRequest::default(),
                &HashMap::new(),
                &metadata,
                None,
            )
            .await;
        assert!(target.is_some());
        if let Some(TargetSpec::List(targets)) = target {
            assert_eq!(targets[0]["model"], "meta/model");
        } else {
            panic!("Expected List target");
        }
    }

    #[tokio::test]
    async fn test_multiple_routes_first_match() {
        let routing = ConditionalRouting {
            pre_request: vec![InterceptorSpec {
                name: "guardrail".to_string(),
                interceptor_type: InterceptorType::Guardrail {
                    guard_id: "guard_id".to_string(),
                },
                extra: HashMap::new(),
            }],
            routes: vec![
                Route {
                    name: "first".to_string(),
                    conditions: Some(RouteCondition::Expr(HashMap::from([(
                        "pre_request.guardrail.result".to_string(),
                        ConditionOp {
                            op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
                        },
                    )]))),
                    targets: Some(TargetSpec::List(vec![HashMap::from([(
                        "model".to_string(),
                        serde_json::json!("first/model"),
                    )])])),
                    message_mapper: None,
                },
                Route {
                    name: "second".to_string(),
                    conditions: Some(RouteCondition::Expr(HashMap::from([(
                        "metadata.region".to_string(),
                        ConditionOp {
                            op: HashMap::from([(ConditionOpType::Eq, serde_json::json!("Europe"))]),
                        },
                    )]))),
                    targets: Some(TargetSpec::List(vec![HashMap::from([(
                        "model".to_string(),
                        serde_json::json!("second/model"),
                    )])])),
                    message_mapper: None,
                },
            ],
            post_request: vec![],
        };
        let router = ConditionalRouter { routing };
        let factory = Box::new(MockFactory { result: true }) as Box<dyn InterceptorFactory>;
        let mut metadata = HashMap::new();
        metadata.insert("region".to_string(), serde_json::json!("Europe"));
        let target = router
            .get_target(
                factory,
                &ChatCompletionRequest::default(),
                &HashMap::new(),
                &metadata,
                None,
            )
            .await;
        assert!(target.is_some());
        if let Some(TargetSpec::List(targets)) = target {
            assert_eq!(targets[0]["model"], "first/model");
        } else {
            panic!("Expected List target");
        }
    }

    #[tokio::test]
    async fn test_no_routes_match() {
        let routing = ConditionalRouting {
            pre_request: vec![InterceptorSpec {
                name: "guardrail".to_string(),
                interceptor_type: InterceptorType::Guardrail {
                    guard_id: "guard_id".to_string(),
                },
                extra: HashMap::new(),
            }],
            routes: vec![Route {
                name: "guarded_route".to_string(),
                conditions: Some(RouteCondition::Expr(HashMap::from([(
                    "pre_request.guardrail.result".to_string(),
                    ConditionOp {
                        op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
                    },
                )]))),
                targets: Some(TargetSpec::List(vec![HashMap::from([(
                    "model".to_string(),
                    serde_json::json!("mock/model"),
                )])])),
                message_mapper: None,
            }],
            post_request: vec![],
        };
        let router = ConditionalRouter { routing };
        let factory = Box::new(MockFactory { result: false }) as Box<dyn InterceptorFactory>;
        let target = router
            .get_target(
                factory,
                &ChatCompletionRequest::default(),
                &HashMap::new(),
                &HashMap::new(),
                None,
            )
            .await;
        assert!(target.is_none());
    }

    #[tokio::test]
    async fn test_lazy_interceptor_execution() {
        // Test that interceptors are only executed when needed
        let routing = ConditionalRouting {
            pre_request: vec![
                InterceptorSpec {
                    name: "guardrail1".to_string(),
                    interceptor_type: InterceptorType::Guardrail {
                        guard_id: "guard_id1".to_string(),
                    },
                    extra: HashMap::new(),
                },
                InterceptorSpec {
                    name: "guardrail2".to_string(),
                    interceptor_type: InterceptorType::Guardrail {
                        guard_id: "guard_id2".to_string(),
                    },
                    extra: HashMap::new(),
                },
            ],
            routes: vec![
                Route {
                    name: "first_route".to_string(),
                    conditions: Some(RouteCondition::Expr(HashMap::from([(
                        "pre_request.guardrail1.result".to_string(),
                        ConditionOp {
                            op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
                        },
                    )]))),
                    targets: Some(TargetSpec::List(vec![HashMap::from([(
                        "model".to_string(),
                        serde_json::json!("first/model"),
                    )])])),
                    message_mapper: None,
                },
                // This route should never be reached because first_route matches
                Route {
                    name: "second_route".to_string(),
                    conditions: Some(RouteCondition::Expr(HashMap::from([(
                        "pre_request.guardrail2.result".to_string(),
                        ConditionOp {
                            op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
                        },
                    )]))),
                    targets: Some(TargetSpec::List(vec![HashMap::from([(
                        "model".to_string(),
                        serde_json::json!("second/model"),
                    )])])),
                    message_mapper: None,
                },
            ],
            post_request: vec![],
        };
        let router = ConditionalRouter { routing };
        let factory = Box::new(MockFactory { result: true }) as Box<dyn InterceptorFactory>;

        let target = router
            .get_target(
                factory,
                &ChatCompletionRequest::default(),
                &HashMap::new(),
                &HashMap::new(),
                None,
            )
            .await;

        assert!(target.is_some());
        if let Some(TargetSpec::List(targets)) = target {
            assert_eq!(targets[0]["model"], "first/model");
        } else {
            panic!("Expected List target");
        }
    }
}
