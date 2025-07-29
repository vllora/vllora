use crate::routing::interceptor::LazyInterceptorManager;
use crate::routing::{ConditionOpType, Route, RouteCondition};
use crate::routing::strategy::conditional::metadata::{MetadataField, MetadataManager, MetadataError};
use crate::types::gateway::Extra;
use std::collections::{HashMap, HashSet};

/// Evaluates if a route's conditions are met, given the context and pre_request results.
pub fn evaluate_conditions(
    condition: &RouteCondition,
    pre_request_results: &HashMap<String, serde_json::Value>,
    metadata: &HashMap<String, serde_json::Value>,
    extra: &Option<Extra>,
) -> bool {
    tracing::warn!("Evaluating conditions: {:#?}", condition);
    tracing::warn!("Pre request results: {:#?}", pre_request_results);

    match condition {
        RouteCondition::All { all } => all
            .iter()
            .all(|expr| evaluate_expr(expr, pre_request_results, metadata, extra)),
        RouteCondition::Any { any } => any
            .iter()
            .any(|expr| evaluate_expr(expr, pre_request_results, metadata, extra)),
        RouteCondition::Expr(map) => map
            .iter()
            .all(|(k, v)| evaluate_op(k, v, pre_request_results, metadata, extra)),
    }
}

/// Evaluates if a route's conditions are met using lazy interceptor execution.
pub async fn evaluate_conditions_lazy(
    condition: &RouteCondition,
    lazy_manager: &mut LazyInterceptorManager,
    metadata: &HashMap<String, serde_json::Value>,
    extra: &Option<Extra>,
) -> Result<bool, crate::routing::interceptor::InterceptorError> {
    tracing::warn!("Evaluating conditions lazily: {:#?}", condition);

    match condition {
        RouteCondition::All { all } => {
            for expr in all {
                if !evaluate_expr_lazy(expr, lazy_manager, metadata, extra).await? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        RouteCondition::Any { any } => {
            for expr in any {
                if evaluate_expr_lazy(expr, lazy_manager, metadata, extra).await? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        RouteCondition::Expr(map) => {
            for (k, v) in map {
                if !evaluate_op_lazy(k, v, lazy_manager, metadata, extra).await? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

fn evaluate_expr(
    expr: &crate::routing::ConditionExpr,
    pre_request_results: &HashMap<String, serde_json::Value>,
    metadata: &HashMap<String, serde_json::Value>,
    extra: &Option<Extra>,
) -> bool {
    match expr {
        crate::routing::ConditionExpr::Expr(map) => map
            .iter()
            .all(|(k, v)| evaluate_op(k, v, pre_request_results, metadata, extra)),
    }
}

async fn evaluate_expr_lazy(
    expr: &crate::routing::ConditionExpr,
    lazy_manager: &mut LazyInterceptorManager,
    metadata: &HashMap<String, serde_json::Value>,
    extra: &Option<Extra>,
) -> Result<bool, crate::routing::interceptor::InterceptorError> {
    match expr {
        crate::routing::ConditionExpr::Expr(map) => {
            for (k, v) in map {
                if !evaluate_op_lazy(k, v, lazy_manager, metadata, extra).await? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

fn evaluate_op(
    key: &str,
    op: &crate::routing::ConditionOp,
    pre_request_results: &HashMap<String, serde_json::Value>,
    metadata: &HashMap<String, serde_json::Value>,
    extra: &Option<Extra>,
) -> bool {
    // Helper function to get the value to compare against
    let get_value = |key: &str| -> Option<serde_json::Value> {
        if key.starts_with("pre_request.") {
            let parts: Vec<&str> = key.split('.').collect();
            if parts.len() == 3 {
                let name = parts[1];
                if let Some(val) = pre_request_results.get(name) {
                    if let Some(obj) = val.as_object() {
                        return obj.get(parts[2]).cloned();
                    }
                }
            }
        } else if let Some(meta_key) = key.strip_prefix("metadata.") {
            return metadata.get(meta_key).cloned();
        } else if key.starts_with("extra.") {
            // Handle Extra field metadata
            let field_str = key.strip_prefix("extra.").unwrap();
            if let Ok(field) = MetadataField::from_string(field_str) {
                if let Ok(Some(value)) = field.extract(extra) {
                    return Some(value);
                }
            }
        }
        None
    };

    // Get the value to compare
    let Some(value) = get_value(key) else {
        return false;
    };

    // Check each operator in the op map
    for (op_name, op_value) in &op.op {
        match op_name {
            ConditionOpType::Eq => {
                if value != *op_value {
                    return false;
                }
            }
            ConditionOpType::Ne => {
                if value == *op_value {
                    return false;
                }
            }
            ConditionOpType::In => {
                if let Some(array) = op_value.as_array() {
                    if !array.contains(&value) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            ConditionOpType::Gt => {
                if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                    if val_num <= op_num {
                        return false;
                    }
                } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                    if val_str <= op_str {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            ConditionOpType::Lt => {
                if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                    if val_num >= op_num {
                        return false;
                    }
                } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                    if val_str >= op_str {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            ConditionOpType::Lte => {
                if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                    if val_num > op_num {
                        return false;
                    }
                } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                    if val_str > op_str {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            ConditionOpType::Gte => {
                if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                    if val_num < op_num {
                        return false;
                    }
                } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                    if val_str < op_str {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }
    }

    true
}

async fn evaluate_op_lazy(
    key: &str,
    op: &crate::routing::ConditionOp,
    lazy_manager: &mut LazyInterceptorManager,
    metadata: &HashMap<String, serde_json::Value>,
    extra: &Option<Extra>,
) -> Result<bool, crate::routing::interceptor::InterceptorError> {
    // Helper function to get the value to compare against
    let get_value = |key: &str| -> Option<serde_json::Value> {
        if key.starts_with("pre_request.") {
            let parts: Vec<&str> = key.split('.').collect();
            if parts.len() == 3 {
                let _name = parts[1];
                // For lazy evaluation, we need to execute the interceptor if not already done
                // This will be handled by the caller
                return None; // We'll handle this differently
            }
        } else if let Some(meta_key) = key.strip_prefix("metadata.") {
            return metadata.get(meta_key).cloned();
        } else if key.starts_with("extra.") {
            // Handle Extra field metadata
            let field_str = key.strip_prefix("extra.").unwrap();
            if let Ok(field) = MetadataField::from_string(field_str) {
                if let Ok(Some(value)) = field.extract(extra) {
                    return Some(value);
                }
            }
        }
        None
    };

    // Get the value to compare
    let value = if key.starts_with("pre_request.") {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() == 3 {
            let interceptor_name = parts[1];
            let field_name = parts[2];
            
            // Execute the interceptor if needed
            let interceptor_result = lazy_manager
                .get_interceptor_result(interceptor_name)
                .await?;
            
            if let Some(result_data) = interceptor_result {
                if let Some(obj) = result_data.as_object() {
                    let field_value = obj.get(field_name).cloned();
                    field_value
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        get_value(key)
    };

    let Some(value) = value else {
        return Ok(false);
    };

    // Check each operator in the op map
    for (op_name, op_value) in &op.op {
        match op_name {
            ConditionOpType::Eq => {
                if value != *op_value {
                    return Ok(false);
                }
            }
            ConditionOpType::Ne => {
                if value == *op_value {
                    return Ok(false);
                }
            }
            ConditionOpType::In => {
                if let Some(array) = op_value.as_array() {
                    if !array.contains(&value) {
                        return Ok(false);
                    }
                } else {
                    return Ok(false);
                }
            }
            ConditionOpType::Gt => {
                if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                    if val_num <= op_num {
                        return Ok(false);
                    }
                } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                    if val_str <= op_str {
                        return Ok(false);
                    }
                } else {
                    return Ok(false);
                }
            }
            ConditionOpType::Lt => {
                if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                    if val_num >= op_num {
                        return Ok(false);
                    }
                } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                    if val_str >= op_str {
                        return Ok(false);
                    }
                } else {
                    return Ok(false);
                }
            }
            ConditionOpType::Lte => {
                if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                    if val_num > op_num {
                        return Ok(false);
                    }
                } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                    if val_str > op_str {
                        return Ok(false);
                    }
                } else {
                    return Ok(false);
                }
            }
            ConditionOpType::Gte => {
                if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                    if val_num < op_num {
                        return Ok(false);
                    }
                } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                    if val_str < op_str {
                        return Ok(false);
                    }
                } else {
                    return Ok(false);
                }
            }
        }
    }
    
    Ok(true)
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
            Ok(serde_json::json!({"result": true}))
        }
        async fn post_request(
            &self,
            _context: &mut InterceptorContext,
            _response: &serde_json::Value,
        ) -> Result<serde_json::Value, InterceptorError> {
            Ok(serde_json::json!({"result": true}))
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
                        op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
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
            if evaluate_conditions(&route.conditions, &pre_request_results, &HashMap::new(, &None), &None) {
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

    #[test]
    fn test_eq_operator() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert("guardrail".to_string(), serde_json::json!({"result": true}));

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.result".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_ne_operator() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert("guardrail".to_string(), serde_json::json!({"result": true}));

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.result".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Ne, serde_json::json!(false))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_in_operator() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"result": "approved"}),
        );

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.result".to_string(),
            ConditionOp {
                op: HashMap::from([(
                    ConditionOpType::In,
                    serde_json::json!(["approved", "pending", "reviewed"]),
                )]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_gt_operator_numeric() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert("guardrail".to_string(), serde_json::json!({"score": 85.5}));

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.score".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Gt, serde_json::json!(80.0))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_gt_operator_string() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"status": "zebra"}),
        );

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.status".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Gt, serde_json::json!("apple"))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_lt_operator_numeric() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert("guardrail".to_string(), serde_json::json!({"score": 75.0}));

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.score".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Lt, serde_json::json!(80.0))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_lt_operator_string() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"status": "low"}),
        );

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.status".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Lt, serde_json::json!("medium"))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_metadata_operators() {
        let mut metadata = HashMap::new();
        metadata.insert("user.tier".to_string(), serde_json::json!("premium"));
        metadata.insert("region".to_string(), serde_json::json!("us-west"));

        // Test eq with metadata
        let condition = RouteCondition::Expr(HashMap::from([(
            "metadata.user.tier".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Eq, serde_json::json!("premium"))]),
            },
        )]));

        assert!(evaluate_conditions(&condition, &HashMap::new(), &metadata), &None);

        // Test in with metadata
        let condition = RouteCondition::Expr(HashMap::from([(
            "metadata.region".to_string(),
            ConditionOp {
                op: HashMap::from([(
                    ConditionOpType::In,
                    serde_json::json!(["us-east", "us-west", "eu-west"]),
                )]),
            },
        )]));

        assert!(evaluate_conditions(&condition, &HashMap::new(), &metadata), &None);
    }

    #[test]
    fn test_multiple_operators() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"score": 85, "status": "approved"}),
        );

        let condition = RouteCondition::Expr(HashMap::from([
            (
                "pre_request.guardrail.score".to_string(),
                ConditionOp {
                    op: HashMap::from([(ConditionOpType::Gt, serde_json::json!(80))]),
                },
            ),
            (
                "pre_request.guardrail.status".to_string(),
                ConditionOp {
                    op: HashMap::from([(ConditionOpType::Eq, serde_json::json!("approved"))]),
                },
            ),
        ]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_false_conditions() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"result": true, "score": 75}),
        );

        // Test ne with false condition
        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.result".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Ne, serde_json::json!(true))]),
            },
        )]));

        assert!(!evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));

        // Test in with false condition
        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.result".to_string(),
            ConditionOp {
                op: HashMap::from([(
                    ConditionOpType::In,
                    serde_json::json!(["rejected", "pending"]),
                )]),
            },
        )]));

        assert!(!evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));

        // Test gt with false condition
        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.score".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Gt, serde_json::json!(80))]),
            },
        )]));

        assert!(!evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_lte_operator_numeric() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert("guardrail".to_string(), serde_json::json!({"score": 75.0}));

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.score".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Lte, serde_json::json!(80.0))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_lte_operator_numeric_equal() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert("guardrail".to_string(), serde_json::json!({"score": 80.0}));

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.score".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Lte, serde_json::json!(80.0))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_lte_operator_string() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"status": "low"}),
        );

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.status".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Lte, serde_json::json!("medium"))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_lte_operator_string_equal() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"status": "medium"}),
        );

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.status".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Lte, serde_json::json!("medium"))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_gte_operator_numeric() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert("guardrail".to_string(), serde_json::json!({"score": 85.0}));

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.score".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Gte, serde_json::json!(80.0))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_gte_operator_numeric_equal() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert("guardrail".to_string(), serde_json::json!({"score": 80.0}));

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.score".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Gte, serde_json::json!(80.0))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_gte_operator_string() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"status": "zebra"}),
        );

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.status".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Gte, serde_json::json!("apple"))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_gte_operator_string_equal() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"status": "medium"}),
        );

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.status".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Gte, serde_json::json!("medium"))]),
            },
        )]));

        assert!(evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));
    }

    #[test]
    fn test_lte_gte_false_conditions() {
        let mut pre_request_results = HashMap::new();
        pre_request_results.insert(
            "guardrail".to_string(),
            serde_json::json!({"score": 85, "status": "apple"}),
        );

        // Test lte with false condition (85 > 80)
        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.score".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Lte, serde_json::json!(80))]),
            },
        )]));

        assert!(!evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new()
        ));

        // Test gte with false condition (apple < zebra)
        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.status".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Gte, serde_json::json!("zebra"))]),
            },
        )]));

        assert!(!evaluate_conditions(
            &condition,
            &pre_request_results,
            &HashMap::new(),
            &None
        ));
    }

    #[test]
    fn test_extra_user_metadata() {
        use crate::types::gateway::{Extra, RequestUser};
        
        let user = RequestUser {
            id: Some("user123".to_string()),
            name: Some("John Doe".to_string()),
            email: Some("john@example.com".to_string()),
            tiers: Some(vec!["premium".to_string(), "enterprise".to_string()]),
        };
        
        let extra = Some(Extra {
            user: Some(user),
            guards: vec![],
            cache: None,
            variables: None,
        });

        // Test user.id
        let condition = RouteCondition::Expr(HashMap::from([
            (
                "extra.user.id".to_string(),
                ConditionOp {
                    op: HashMap::from([
                        (ConditionOpType::Eq, serde_json::Value::String("user123".to_string())),
                    ]),
                },
            ),
        ]));

        assert!(evaluate_conditions(&condition, &HashMap::new(), &HashMap::new(, &None), &extra));

        // Test user.tiers with in operator
        let condition = RouteCondition::Expr(HashMap::from([
            (
                "extra.user.tiers".to_string(),
                ConditionOp {
                    op: HashMap::from([
                        (ConditionOpType::In, serde_json::Value::String("premium".to_string())),
                    ]),
                },
            ),
        ]));

        assert!(evaluate_conditions(&condition, &HashMap::new(), &HashMap::new(, &None), &extra));
    }

    #[test]
    fn test_extra_variables_metadata() {
        use crate::types::gateway::{Extra, RequestUser};
        use std::collections::HashMap;
        
        let mut variables = HashMap::new();
        variables.insert("priority".to_string(), serde_json::Value::String("high".to_string()));
        variables.insert("budget".to_string(), serde_json::Value::Number(serde_json::Number::from(1000)));
        
        let extra = Some(Extra {
            user: None,
            guards: vec![],
            cache: None,
            variables: Some(variables),
        });

        // Test variables.priority
        let condition = RouteCondition::Expr(HashMap::from([
            (
                "extra.variables.priority".to_string(),
                ConditionOp {
                    op: HashMap::from([
                        (ConditionOpType::Eq, serde_json::Value::String("high".to_string())),
                    ]),
                },
            ),
        ]));

        assert!(evaluate_conditions(&condition, &HashMap::new(), &HashMap::new(, &None), &extra));

        // Test variables.budget with numeric comparison
        let condition = RouteCondition::Expr(HashMap::from([
            (
                "extra.variables.budget".to_string(),
                ConditionOp {
                    op: HashMap::from([
                        (ConditionOpType::Gt, serde_json::Value::Number(serde_json::Number::from(500))),
                    ]),
                },
            ),
        ]));

        assert!(evaluate_conditions(&condition, &HashMap::new(), &HashMap::new(, &None), &extra));
    }
}
