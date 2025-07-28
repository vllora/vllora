use crate::routing::{Route, RouteCondition};
use std::collections::{HashMap, HashSet};

/// Evaluates if a route's conditions are met, given the context and pre_request results.
pub fn evaluate_conditions(
    condition: &RouteCondition,
    pre_request_results: &HashMap<String, serde_json::Value>,
    metadata: &HashMap<String, serde_json::Value>,
) -> bool {
    tracing::warn!("Evaluating conditions: {:#?}", condition);
    tracing::warn!("Pre request results: {:#?}", pre_request_results);

    match condition {
        RouteCondition::All { all } => all
            .iter()
            .all(|expr| evaluate_expr(expr, pre_request_results, metadata)),
        RouteCondition::Any { any } => any
            .iter()
            .any(|expr| evaluate_expr(expr, pre_request_results, metadata)),
        RouteCondition::Expr(map) => map
            .iter()
            .all(|(k, v)| evaluate_op(k, v, pre_request_results, metadata)),
    }
}

fn evaluate_expr(
    expr: &crate::routing::ConditionExpr,
    pre_request_results: &HashMap<String, serde_json::Value>,
    metadata: &HashMap<String, serde_json::Value>,
) -> bool {
    match expr {
        crate::routing::ConditionExpr::Expr(map) => map
            .iter()
            .all(|(k, v)| evaluate_op(k, v, pre_request_results, metadata)),
    }
}

fn evaluate_op(
    key: &str,
    op: &crate::routing::ConditionOp,
    pre_request_results: &HashMap<String, serde_json::Value>,
    metadata: &HashMap<String, serde_json::Value>,
) -> bool {
    // Only support eq for pre_request.guardrail.result and metadata for test
    if key.starts_with("pre_request.") {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() == 3 {
            let name = parts[1];
            tracing::warn!("Pre request results: {:#?}", pre_request_results);
            tracing::warn!("Name: {}", name);
            if let Some(val) = pre_request_results.get(name) {
                tracing::warn!("Comparing pre_request.{} with {}", name, val);
                if let Some(t) = val.as_object().unwrap().get(parts[2]) {
                    tracing::warn!("T: {:#?}", t);
                    tracing::warn!("Op: {:#?}", op.op.get("eq"));
                    if let Some(eq) = op.op.get("eq") {
                        return t == eq;
                    }
                }
            }
        }
    } else if let Some(meta_key) = key.strip_prefix("metadata.") {
        if let Some(val) = metadata.get(meta_key) {
            if let Some(eq) = op.op.get("eq") {
                return val == eq;
            }
        }
    }
    false
}

/// Returns the set of pre_request interceptor names referenced in any route condition
pub fn referenced_pre_request_interceptors(routes: &[Route]) -> HashSet<String> {
    let mut set = HashSet::new();
    for route in routes {
        collect_pre_request_keys(&route.conditions, &mut set);
    }
    tracing::warn!("Referenced pre request interceptors: {:#?}", set);
    set
}

fn collect_pre_request_keys(cond: &RouteCondition, set: &mut HashSet<String>) {
    match cond {
        RouteCondition::All { all } => {
            for expr in all {
                let crate::routing::ConditionExpr::Expr(map) = expr;
                for k in map.keys() {
                    if k.starts_with("pre_request.") {
                        let parts: Vec<&str> = k.split('.').collect();
                        if parts.len() == 3 {
                            set.insert(parts[1].to_string());
                        }
                    }
                }
            }
        }
        RouteCondition::Any { any } => {
            for expr in any {
                let crate::routing::ConditionExpr::Expr(map) = expr;
                for k in map.keys() {
                    if k.starts_with("pre_request.") {
                        let parts: Vec<&str> = k.split('.').collect();
                        if parts.len() == 3 {
                            set.insert(parts[1].to_string());
                        }
                    }
                }
            }
        }
        RouteCondition::Expr(map) => {
            for k in map.keys() {
                if k.starts_with("pre_request.") {
                    let parts: Vec<&str> = k.split('.').collect();
                    if parts.len() == 3 {
                        set.insert(parts[1].to_string());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::interceptor::{
        Interceptor, InterceptorContext, InterceptorError, InterceptorState,
    };
    use crate::routing::{
        ConditionOp, ConditionalRouting, InterceptorSpec, InterceptorType, Route, RouteCondition,
        TargetSpec,
    };
    use crate::types::gateway::ChatCompletionRequest;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MockGuardrail;
    #[async_trait::async_trait]
    impl Interceptor for MockGuardrail {
        fn name(&self) -> &str {
            "guardrail"
        }
        async fn pre_request(
            &self,
            _context: &mut InterceptorContext,
        ) -> Result<serde_json::Value, InterceptorError> {
            Ok(serde_json::json!(true))
        }
        async fn post_request(
            &self,
            _context: &mut InterceptorContext,
            _response: &serde_json::Value,
        ) -> Result<serde_json::Value, InterceptorError> {
            Ok(serde_json::json!(true))
        }
    }

    #[tokio::test]
    async fn test_conditional_router_guardrail() {
        // Setup a simple routing config with a guardrail pre_request
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
                conditions: RouteCondition::Expr(HashMap::from([(
                    "pre_request.guardrail.result".to_string(),
                    ConditionOp {
                        op: HashMap::from([("eq".to_string(), serde_json::json!(true))]),
                    },
                )])),
                targets: Some(TargetSpec::List(vec![HashMap::from([(
                    "model".to_string(),
                    serde_json::json!("mock/model"),
                )])])),
                message_mapper: None,
            }],
            post_request: vec![],
        };

        // Only run interceptors referenced in conditions
        let referenced = referenced_pre_request_interceptors(&routing.routes);
        let mut pre_request_results = HashMap::new();
        let mut interceptors: HashMap<String, Arc<dyn Interceptor>> = HashMap::new();
        interceptors.insert("guardrail".to_string(), Arc::new(MockGuardrail));

        // Simulate running only referenced interceptors
        for name in referenced {
            if let Some(interceptor) = interceptors.get(&name) {
                let state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
                let mut context = InterceptorContext::new(
                    ChatCompletionRequest::default(),
                    HashMap::new(),
                    state,
                );
                let result = interceptor.pre_request(&mut context).await.unwrap();
                pre_request_results.insert(name, result);
            }
        }

        // Evaluate routes
        let mut selected = None;
        for route in &routing.routes {
            if evaluate_conditions(&route.conditions, &pre_request_results, &HashMap::new()) {
                selected = Some(route);
                break;
            }
        }
        assert!(selected.is_some());
        let route = selected.unwrap();
        assert_eq!(route.name, "guarded_route");
        if let Some(TargetSpec::List(targets)) = &route.targets {
            assert_eq!(targets[0]["model"], "mock/model");
        } else {
            panic!("Expected List target");
        }
    }
}
