use crate::routing::interceptor::LazyInterceptorManager;
use crate::routing::strategy::conditional::metadata::MetadataField;
use crate::routing::{ConditionOpType, Route, RouteCondition};
use std::collections::{HashMap, HashSet};
use vllora_llm::types::gateway::Extra;

/// Evaluates if a route's conditions are met using lazy interceptor execution.
pub async fn evaluate_conditions(
    condition: &RouteCondition,
    lazy_manager: &mut LazyInterceptorManager,
    metadata: &HashMap<String, serde_json::Value>,
    extra: Option<&Extra>,
) -> Result<bool, crate::routing::interceptor::InterceptorError> {
    match condition {
        RouteCondition::All { all } => {
            for expr in all {
                if !evaluate_expr(expr, lazy_manager, metadata, extra).await? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        RouteCondition::Any { any } => {
            for expr in any {
                if evaluate_expr(expr, lazy_manager, metadata, extra).await? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        RouteCondition::Expr(map) => {
            for (k, v) in map {
                if !evaluate_op(k, v, lazy_manager, metadata, extra).await? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

async fn evaluate_expr(
    expr: &crate::routing::ConditionExpr,
    lazy_manager: &mut LazyInterceptorManager,
    metadata: &HashMap<String, serde_json::Value>,
    extra: Option<&Extra>,
) -> Result<bool, crate::routing::interceptor::InterceptorError> {
    match expr {
        crate::routing::ConditionExpr::Expr(map) => {
            for (k, v) in map {
                if !evaluate_op(k, v, lazy_manager, metadata, extra).await? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

async fn evaluate_op(
    key: &str,
    op: &crate::routing::ConditionOp,
    lazy_manager: &mut LazyInterceptorManager,
    metadata: &HashMap<String, serde_json::Value>,
    extra: Option<&Extra>,
) -> Result<bool, crate::routing::interceptor::InterceptorError> {
    let get_value = |key: &str| -> Option<serde_json::Value> {
        if key.starts_with("pre_request.") {
            let parts: Vec<&str> = key.split('.').collect();
            if parts.len() == 3 {
                // For lazy evaluation, we need to execute the interceptor if not already done
                // This will be handled by the caller
                return None;
            }
        } else if let Some(meta_key) = key.strip_prefix("metadata.") {
            return metadata.get(meta_key).cloned();
        } else if key.starts_with("extra.") {
            let field_str = key.strip_prefix("extra.").unwrap();
            if let Ok(field) = MetadataField::from_string(field_str) {
                if let Ok(Some(value)) = field.extract(extra) {
                    return Some(value);
                }
            }
        }
        None
    };

    let value = if key.starts_with("pre_request.") {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() == 3 {
            let interceptor_name = parts[1];
            let field_name = parts[2];
            let interceptor_result = lazy_manager
                .get_interceptor_result(interceptor_name)
                .await?;
            if let Some(result_data) = interceptor_result {
                if let Some(obj) = result_data.as_object() {
                    obj.get(field_name).cloned()
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

    for (op_name, op_value) in &op.op {
        if !compare_values(op_name, op_value, &value) {
            return Ok(false);
        }
    }

    Ok(true)
}

pub fn compare_values(
    condition_op: &ConditionOpType,
    op_value: &serde_json::Value,
    value: &serde_json::Value,
) -> bool {
    match condition_op {
        ConditionOpType::Eq => *value == *op_value,
        ConditionOpType::Ne => *value != *op_value,
        ConditionOpType::In => {
            if let Some(array) = op_value.as_array() {
                array.contains(value)
            } else {
                false
            }
        }
        ConditionOpType::Gt => {
            if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                val_num > op_num
            } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                val_str > op_str
            } else {
                false
            }
        }
        ConditionOpType::Lt => {
            if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                val_num < op_num
            } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                val_str < op_str
            } else {
                false
            }
        }
        ConditionOpType::Lte => {
            if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                val_num <= op_num
            } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                val_str <= op_str
            } else {
                false
            }
        }
        ConditionOpType::Gte => {
            if let (Some(val_num), Some(op_num)) = (value.as_f64(), op_value.as_f64()) {
                val_num >= op_num
            } else if let (Some(val_str), Some(op_str)) = (value.as_str(), op_value.as_str()) {
                val_str >= op_str
            } else {
                false
            }
        }
        ConditionOpType::Contains => {
            if let Some(vec) = value.as_array() {
                vec.contains(op_value)
            } else {
                false
            }
        }
    }
}

/// Returns the set of pre_request interceptor names referenced in any route condition
pub fn referenced_pre_request_interceptors(routes: &[Route]) -> HashSet<String> {
    let mut set = HashSet::new();
    for route in routes {
        if let Some(conditions) = &route.conditions {
            collect_pre_request_keys(conditions, &mut set);
        }
    }
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
        Interceptor, InterceptorContext, InterceptorError, InterceptorState, LazyInterceptorManager,
    };
    use crate::routing::{ConditionOp, RouteCondition};
    use vllora_llm::types::gateway::ChatCompletionRequest;
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
            Ok(serde_json::json!({"result": true, "score": 85.5, "status": "approved"}))
        }
        async fn post_request(
            &self,
            _context: &mut InterceptorContext,
            _response: &serde_json::Value,
        ) -> Result<serde_json::Value, InterceptorError> {
            Ok(serde_json::json!({"result": true}))
        }
    }

    async fn setup_lazy_manager(
        interceptors: HashMap<String, Arc<dyn Interceptor>>,
    ) -> LazyInterceptorManager {
        LazyInterceptorManager::new(
            interceptors,
            InterceptorContext::new(
                ChatCompletionRequest::default(),
                None,
                HashMap::new(),
                Arc::new(tokio::sync::RwLock::new(InterceptorState::new())),
            ),
        )
    }

    #[tokio::test]
    async fn test_eq_operator_lazy() {
        let mut interceptors: HashMap<String, Arc<dyn Interceptor>> = HashMap::new();
        interceptors.insert("guardrail".to_string(), Arc::new(MockGuardrail));
        let mut lazy_manager = setup_lazy_manager(interceptors).await;

        let condition = RouteCondition::Expr(HashMap::from([(
            "pre_request.guardrail.result".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Eq, serde_json::json!(true))]),
            },
        )]));

        assert!(
            evaluate_conditions(&condition, &mut lazy_manager, &HashMap::new(), None)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_metadata_operator_lazy() {
        let interceptors: HashMap<String, Arc<dyn Interceptor>> = HashMap::new();
        let mut lazy_manager = setup_lazy_manager(interceptors).await;
        let mut metadata = HashMap::new();
        metadata.insert("user.tier".to_string(), serde_json::json!("premium"));

        let condition = RouteCondition::Expr(HashMap::from([(
            "metadata.user.tier".to_string(),
            ConditionOp {
                op: HashMap::from([(ConditionOpType::Eq, serde_json::json!("premium"))]),
            },
        )]));

        assert!(
            evaluate_conditions(&condition, &mut lazy_manager, &metadata, None)
                .await
                .unwrap()
        );
    }
}
