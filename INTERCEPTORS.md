# LangDB AI Gateway - Interceptor System

## Overview

The LangDB AI Gateway interceptor system provides a powerful way to handle pre_request and post_request operations during the routing process. Interceptors allow you to:

- Log and monitor requests
- Validate and transform requests
- Cache responses
- Collect metrics and analytics
- Implement custom business logic

## Core Components

### Interceptor Trait

The `Interceptor` trait defines the interface for all interceptors:

```rust
#[async_trait::async_trait]
pub trait Interceptor: Send + Sync {
    /// Name of the interceptor
    fn name(&self) -> &str;
    
    /// Execute pre-request interceptor
    async fn pre_request(&self, context: &mut InterceptorContext) -> Result<serde_json::Value, InterceptorError>;
    
    /// Execute post-request interceptor
    async fn post_request(&self, context: &mut InterceptorContext, response: &serde_json::Value) -> Result<serde_json::Value, InterceptorError>;
    
    /// Optional: Validate interceptor configuration
    fn validate_config(&self) -> Result<(), InterceptorError> {
        Ok(())
    }
    
    /// Optional: Check if interceptor should be enabled for this request
    fn should_execute(&self, context: &InterceptorContext) -> bool {
        true
    }
}
```

### InterceptorState

The `InterceptorState` stores all interceptor results and makes them available throughout the routing process:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterceptorState {
    pub pre_request_results: Vec<InterceptorResult>,
    pub post_request_results: Vec<InterceptorResult>,
    pub request_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

### InterceptorContext

The `InterceptorContext` provides access to request data and shared state:

```rust
#[derive(Debug, Clone)]
pub struct InterceptorContext {
    pub request: ChatCompletionRequest,
    pub headers: HashMap<String, String>,
    pub state: Arc<tokio::sync::RwLock<InterceptorState>>,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

## Usage Example

### 1. Create an Interceptor

```rust
use crate::routing::interceptor::{Interceptor, InterceptorContext, InterceptorError};

struct CustomInterceptor {
    name: String,
}

#[async_trait::async_trait]
impl Interceptor for CustomInterceptor {
    fn name(&self) -> &str {
        &self.name
    }

    async fn pre_request(&self, context: &mut InterceptorContext) -> Result<serde_json::Value, InterceptorError> {
        // Pre-request logic here
        tracing::info!("Processing request for model: {}", context.request.model);
        
        Ok(serde_json::json!({
            "interceptor": self.name,
            "model": context.request.model,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        }))
    }

    async fn post_request(&self, context: &mut InterceptorContext, response: &serde_json::Value) -> Result<serde_json::Value, InterceptorError> {
        // Post-request logic here
        tracing::info!("Processed response for model: {}", context.request.model);
        
        Ok(serde_json::json!({
            "interceptor": self.name,
            "response_processed": true
        }))
    }
}
```

### 2. Register Interceptors with Router

```rust
use crate::routing::{LlmRouter, RoutingStrategy};
use crate::routing::interceptor::InterceptorManager;
use std::sync::Arc;

// Create interceptor manager
let mut interceptor_manager = InterceptorManager::new();

// Add interceptors
interceptor_manager.add_interceptor(Arc::new(CustomInterceptor {
    name: "custom_interceptor".to_string(),
}))?;

// Create router with interceptor manager
let router = LlmRouter::new("dynamic_router".to_string(), RoutingStrategy::Fallback)
    .with_interceptor_manager(Arc::new(interceptor_manager));
```

### 3. Access Interceptor State

```rust
// During routing, interceptor state is available
let routing_result = router.route(request, &available_models, headers, &metrics_repository).await?;

if let Some(state) = routing_result.interceptor_state {
    let state_read = state.read().await;
    
    // Access pre-request results
    for result in &state_read.pre_request_results {
        println!("Pre-request interceptor: {} executed in {}ms", 
                 result.interceptor_name, result.execution_time_ms);
    }
    
    // Access specific interceptor data
    if let Some(data) = state_read.get_pre_request_data("custom_interceptor") {
        println!("Custom interceptor data: {}", data);
    }
}
```

## Built-in Interceptor Examples

### LoggingInterceptor

Logs request and response details:

```rust
use crate::routing::interceptor::examples::LoggingInterceptor;

let logger = LoggingInterceptor::new("request_logger".to_string(), "info".to_string());
interceptor_manager.add_interceptor(Arc::new(logger))?;
```

### ValidationInterceptor

Validates requests against configured rules:

```rust
use crate::routing::interceptor::examples::ValidationInterceptor;

let validator = ValidationInterceptor::new(
    "validator".to_string(),
    Some(4096), // max tokens
    vec!["gpt-4".to_string(), "gpt-3.5-turbo".to_string()], // allowed models
    vec!["authorization".to_string()], // required headers
);
interceptor_manager.add_interceptor(Arc::new(validator))?;
```

### MetricsInterceptor

Collects metrics about requests:

```rust
use crate::routing::interceptor::examples::MetricsInterceptor;

let metrics = MetricsInterceptor::new("metrics_collector".to_string(), true, true);
interceptor_manager.add_interceptor(Arc::new(metrics))?;
```

### CachingInterceptor

Caches responses to improve performance:

```rust
use crate::routing::interceptor::examples::CachingInterceptor;

let cache = CachingInterceptor::new("response_cache".to_string(), 300); // 5 minute TTL
interceptor_manager.add_interceptor(Arc::new(cache))?;
```

## Configuration

Interceptors can be configured via the router configuration:

```json
{
    "model": "router/dynamic",
    "router": {
        "name": "advanced_router",
        "type": "optimized",
        "metric": "latency",
        "targets": [
            {"model": "gpt-4", "temperature": 0.7}
        ],
        "interceptors": [
            {
                "name": "logger",
                "type": "logging",
                "log_level": "info"
            },
            {
                "name": "validator",
                "type": "validation",
                "max_tokens": 4096,
                "allowed_models": ["gpt-4", "gpt-3.5-turbo"]
            }
        ]
    }
}
```

## Error Handling

Interceptors can return errors that will be handled by the routing system:

```rust
async fn pre_request(&self, context: &mut InterceptorContext) -> Result<serde_json::Value, InterceptorError> {
    if context.request.model.is_empty() {
        return Err(InterceptorError::ValidationError("Model cannot be empty".to_string()));
    }
    
    // ... rest of interceptor logic
}
```

## Performance Considerations

- Interceptors are executed sequentially, so keep them fast
- Use async operations where possible
- Consider using `should_execute()` to skip unnecessary interceptors
- Store minimal data in the interceptor state
- Use proper logging levels to avoid performance impact

## Best Practices

1. **Keep interceptors focused**: Each interceptor should have a single responsibility
2. **Handle errors gracefully**: Always provide meaningful error messages
3. **Use appropriate logging**: Use structured logging for better observability
4. **Test thoroughly**: Write comprehensive tests for your interceptors
5. **Document configuration**: Provide clear documentation for interceptor settings
6. **Monitor performance**: Track interceptor execution times and impact

## Advanced Usage

### Conditional Execution

```rust
fn should_execute(&self, context: &InterceptorContext) -> bool {
    // Only execute for specific models
    context.request.model.starts_with("gpt-4")
}
```

### Sharing Data Between Interceptors

```rust
async fn pre_request(&self, context: &mut InterceptorContext) -> Result<serde_json::Value, InterceptorError> {
    // Store data for other interceptors
    let mut state = context.state.write().await;
    state.set_metadata("custom_key".to_string(), serde_json::json!({"data": "value"}));
    
    Ok(serde_json::json!({"success": true}))
}
```

### Accessing Previous Interceptor Results

```rust
async fn post_request(&self, context: &mut InterceptorContext, response: &serde_json::Value) -> Result<serde_json::Value, InterceptorError> {
    let state = context.state.read().await;
    
    // Access data from previous interceptors
    if let Some(pre_data) = state.get_pre_request_data("other_interceptor") {
        // Use data from other interceptor
        println!("Other interceptor data: {}", pre_data);
    }
    
    Ok(serde_json::json!({"processed": true}))
}
```

## Testing

Example test for a custom interceptor:

```rust
#[tokio::test]
async fn test_custom_interceptor() {
    let interceptor = CustomInterceptor {
        name: "test".to_string(),
    };
    
    let state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
    let request = ChatCompletionRequest::default();
    let headers = HashMap::new();
    
    let mut context = InterceptorContext::new(request, headers, state.clone());
    
    let result = interceptor.pre_request(&mut context).await;
    assert!(result.is_ok());
    
    let data = result.unwrap();
    assert_eq!(data["interceptor"], "test");
}
```

## Conclusion

The interceptor system provides a powerful and flexible way to extend the LangDB AI Gateway's routing capabilities. By implementing custom interceptors, you can add logging, validation, caching, metrics collection, and other cross-cutting concerns to your AI routing infrastructure.