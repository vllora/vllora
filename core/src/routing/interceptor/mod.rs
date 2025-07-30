use crate::routing::InterceptorSpec;
use crate::types::gateway::ChatCompletionRequest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

mod factory;
pub mod guard;
pub mod transformer;
pub mod types;

pub use factory::RouterInterceptorFactory;

#[derive(Error, Debug)]
pub enum InterceptorError {
    #[error("Interceptor execution failed: {0}")]
    ExecutionError(String),

    #[error("Interceptor state serialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Interceptor validation failed: {0}")]
    ValidationError(String),
}

/// Result of an interceptor execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptorResult {
    pub interceptor_name: String,
    pub execution_time_ms: u64,
    pub data: serde_json::Value,
    pub success: bool,
    pub error: Option<String>,
}

/// State that holds interceptor results
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InterceptorState {
    pub pre_request_results: Vec<InterceptorResult>,
    pub post_request_results: Vec<InterceptorResult>,
    pub request_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl InterceptorState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn add_pre_request_result(&mut self, result: InterceptorResult) {
        self.pre_request_results.push(result);
    }

    pub fn add_post_request_result(&mut self, result: InterceptorResult) {
        self.post_request_results.push(result);
    }

    pub fn get_pre_request_data(&self, interceptor_name: &str) -> Option<&serde_json::Value> {
        self.pre_request_results
            .iter()
            .find(|r| r.interceptor_name == interceptor_name)
            .map(|r| &r.data)
    }

    pub fn get_post_request_data(&self, interceptor_name: &str) -> Option<&serde_json::Value> {
        self.post_request_results
            .iter()
            .find(|r| r.interceptor_name == interceptor_name)
            .map(|r| &r.data)
    }

    pub fn set_metadata(&mut self, key: String, value: serde_json::Value) {
        self.metadata.insert(key, value);
    }

    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }
}

/// Enhanced context passed to interceptors with rich metadata support
#[derive(Debug, Clone)]
pub struct InterceptorContext {
    pub request: ChatCompletionRequest,
    pub headers: HashMap<String, String>,
    pub state: Arc<tokio::sync::RwLock<InterceptorState>>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub extra: Option<crate::types::gateway::Extra>,
    pub chain_position: usize,
    pub results: HashMap<String, InterceptorResult>,
}

impl InterceptorContext {
    pub fn new(
        request: ChatCompletionRequest,
        headers: HashMap<String, String>,
        state: Arc<tokio::sync::RwLock<InterceptorState>>,
    ) -> Self {
        Self {
            request,
            headers,
            state,
            metadata: HashMap::new(),
            extra: None,
            chain_position: 0,
            results: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_extra(mut self, extra: Option<crate::types::gateway::Extra>) -> Self {
        self.extra = extra;
        self
    }

    pub fn with_chain_position(mut self, position: usize) -> Self {
        self.chain_position = position;
        self
    }

    pub fn add_result(&mut self, name: String, result: InterceptorResult) {
        self.results.insert(name, result);
    }

    pub fn get_result(&self, name: &str) -> Option<&InterceptorResult> {
        self.results.get(name)
    }

    pub fn get_extra_metadata(&self) -> Option<&crate::types::gateway::Extra> {
        self.extra.as_ref()
    }
}

/// Trait for implementing interceptors
#[async_trait::async_trait]
pub trait Interceptor: Send + Sync {
    /// Name of the interceptor
    fn name(&self) -> &str;

    /// Execute pre-request interceptor
    async fn pre_request(
        &self,
        context: &mut InterceptorContext,
    ) -> Result<serde_json::Value, InterceptorError>;

    /// Execute post-request interceptor
    async fn post_request(
        &self,
        context: &mut InterceptorContext,
        response: &serde_json::Value,
    ) -> Result<serde_json::Value, InterceptorError>;

    /// Optional: Validate interceptor configuration
    fn validate_config(&self) -> Result<(), InterceptorError> {
        Ok(())
    }

    /// Optional: Check if interceptor should be enabled for this request
    fn should_execute(&self, _context: &InterceptorContext) -> bool {
        true
    }
}

/// Factory trait for creating interceptors from InterceptorSpec
pub trait InterceptorFactory: Send + Sync {
    /// Create an interceptor instance from the given InterceptorSpec
    fn create_interceptor(
        &self,
        spec: &InterceptorSpec,
    ) -> Result<Arc<dyn Interceptor>, InterceptorError>;
}

/// Lazy interceptor manager that only executes interceptors when needed
pub struct LazyInterceptorManager {
    interceptors: HashMap<String, Arc<dyn Interceptor>>,
    state: Arc<tokio::sync::RwLock<InterceptorState>>,
    context: InterceptorContext,
}

impl LazyInterceptorManager {
    pub fn new(
        interceptors: HashMap<String, Arc<dyn Interceptor>>,
        context: InterceptorContext,
    ) -> Self {
        Self {
            interceptors,
            state: context.state.clone(),
            context,
        }
    }

    /// Get the result of a specific interceptor, executing it if not already executed
    pub async fn get_interceptor_result(
        &mut self,
        interceptor_name: &str,
    ) -> Result<Option<serde_json::Value>, InterceptorError> {
        // First check if the result is already available
        {
            let state = self.state.read().await;
            if let Some(data) = state.get_pre_request_data(interceptor_name) {
                return Ok(Some(data.clone()));
            }
        }

        // If not available, execute the interceptor
        if let Some(interceptor) = self.interceptors.get(interceptor_name) {
            let interceptor_start = std::time::Instant::now();

            let (result, data_result) = match interceptor.pre_request(&mut self.context).await {
                Ok(data) => {
                    let data_clone = data.clone();
                    let result = InterceptorResult {
                        interceptor_name: interceptor_name.to_string(),
                        execution_time_ms: interceptor_start.elapsed().as_millis() as u64,
                        data,
                        success: true,
                        error: None,
                    };
                    (result, Ok(Some(data_clone)))
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    let result = InterceptorResult {
                        interceptor_name: interceptor_name.to_string(),
                        execution_time_ms: interceptor_start.elapsed().as_millis() as u64,
                        data: serde_json::Value::Null,
                        success: false,
                        error: Some(error_msg.clone()),
                    };
                    (result, Err(InterceptorError::ExecutionError(error_msg)))
                }
            };

            let mut state = self.state.write().await;
            state.add_pre_request_result(result);

            data_result
        } else {
            Ok(None)
        }
    }

    /// Get all pre-request results that have been executed so far
    pub async fn get_all_results(&self) -> HashMap<String, serde_json::Value> {
        let state = self.state.read().await;
        let mut results = HashMap::new();
        for result in &state.pre_request_results {
            results.insert(result.interceptor_name.clone(), result.data.clone());
        }
        results
    }
}

/// Manager for handling multiple interceptors
pub struct InterceptorManager {
    interceptors: Vec<Arc<dyn Interceptor>>,
}

impl InterceptorManager {
    pub fn new() -> Self {
        Self {
            interceptors: Vec::new(),
        }
    }

    pub fn add_interceptor(
        &mut self,
        interceptor: Arc<dyn Interceptor>,
    ) -> Result<(), InterceptorError> {
        interceptor.validate_config()?;
        self.interceptors.push(interceptor);
        Ok(())
    }

    pub async fn execute_pre_request(
        &self,
        context: &mut InterceptorContext,
    ) -> Result<(), InterceptorError> {
        let start_time = std::time::Instant::now();

        for interceptor in &self.interceptors {
            if !interceptor.should_execute(context) {
                continue;
            }

            let interceptor_start = std::time::Instant::now();
            let result = match interceptor.pre_request(context).await {
                Ok(data) => InterceptorResult {
                    interceptor_name: interceptor.name().to_string(),
                    execution_time_ms: interceptor_start.elapsed().as_millis() as u64,
                    data,
                    success: true,
                    error: None,
                },
                Err(e) => InterceptorResult {
                    interceptor_name: interceptor.name().to_string(),
                    execution_time_ms: interceptor_start.elapsed().as_millis() as u64,
                    data: serde_json::Value::Null,
                    success: false,
                    error: Some(e.to_string()),
                },
            };

            let mut state = context.state.write().await;
            state.add_pre_request_result(result);
        }

        tracing::warn!(
            "Pre-request interceptors executed in {}ms",
            start_time.elapsed().as_millis()
        );
        Ok(())
    }

    pub async fn execute_post_request(
        &self,
        context: &mut InterceptorContext,
        response: &serde_json::Value,
    ) -> Result<(), InterceptorError> {
        let start_time = std::time::Instant::now();

        for interceptor in &self.interceptors {
            if !interceptor.should_execute(context) {
                continue;
            }

            let interceptor_start = std::time::Instant::now();
            let result = match interceptor.post_request(context, response).await {
                Ok(data) => InterceptorResult {
                    interceptor_name: interceptor.name().to_string(),
                    execution_time_ms: interceptor_start.elapsed().as_millis() as u64,
                    data,
                    success: true,
                    error: None,
                },
                Err(e) => InterceptorResult {
                    interceptor_name: interceptor.name().to_string(),
                    execution_time_ms: interceptor_start.elapsed().as_millis() as u64,
                    data: serde_json::Value::Null,
                    success: false,
                    error: Some(e.to_string()),
                },
            };

            let mut state = context.state.write().await;
            state.add_post_request_result(result);
        }

        tracing::debug!(
            "Post-request interceptors executed in {}ms",
            start_time.elapsed().as_millis()
        );
        Ok(())
    }
}

impl Default for InterceptorManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{routing::InterceptorType, types::gateway::ChatCompletionRequest};

    struct TestInterceptor {
        name: String,
    }

    impl TestInterceptor {
        fn new(name: String) -> Self {
            Self { name }
        }
    }

    #[async_trait::async_trait]
    impl Interceptor for TestInterceptor {
        fn name(&self) -> &str {
            &self.name
        }

        async fn pre_request(
            &self,
            context: &mut InterceptorContext,
        ) -> Result<serde_json::Value, InterceptorError> {
            Ok(serde_json::json!({
                "interceptor": self.name,
                "type": "pre_request",
                "request_model": context.request.model
            }))
        }

        async fn post_request(
            &self,
            _context: &mut InterceptorContext,
            _response: &serde_json::Value,
        ) -> Result<serde_json::Value, InterceptorError> {
            Ok(serde_json::json!({
                "interceptor": self.name,
                "type": "post_request",
                "response_processed": true
            }))
        }
    }

    #[tokio::test]
    async fn test_interceptor_state() {
        let mut state = InterceptorState::new();

        let result = InterceptorResult {
            interceptor_name: "test".to_string(),
            execution_time_ms: 100,
            data: serde_json::json!({"test": "value"}),
            success: true,
            error: None,
        };

        state.add_pre_request_result(result);

        let retrieved = state.get_pre_request_data("test").unwrap();
        assert_eq!(retrieved["test"], "value");
    }

    #[tokio::test]
    async fn test_interceptor_manager() {
        let mut manager = InterceptorManager::new();
        let interceptor = Arc::new(TestInterceptor::new("test_interceptor".to_string()));

        manager.add_interceptor(interceptor).unwrap();

        let state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
        let request = ChatCompletionRequest::default();
        let headers = HashMap::new();

        let mut context = InterceptorContext::new(request, headers, state.clone());

        manager.execute_pre_request(&mut context).await.unwrap();

        let state_read = state.read().await;
        assert_eq!(state_read.pre_request_results.len(), 1);
        assert_eq!(
            state_read.pre_request_results[0].interceptor_name,
            "test_interceptor"
        );
        assert!(state_read.pre_request_results[0].success);
    }

    #[tokio::test]
    async fn test_lazy_interceptor_manager() {
        let interceptor = Arc::new(TestInterceptor::new("lazy_test".to_string()));
        let mut interceptors = HashMap::new();
        interceptors.insert("lazy_test".to_string(), interceptor as Arc<dyn Interceptor>);

        let state = Arc::new(tokio::sync::RwLock::new(InterceptorState::new()));
        let request = ChatCompletionRequest::default();
        let headers = HashMap::new();
        let context = InterceptorContext::new(request, headers, state.clone());

        let mut lazy_manager = LazyInterceptorManager::new(interceptors, context);

        // Initially no results
        let initial_results = lazy_manager.get_all_results().await;
        assert!(initial_results.is_empty());

        // Get result for the first time - should execute
        let result = lazy_manager
            .get_interceptor_result("lazy_test")
            .await
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap()["interceptor"], "lazy_test");

        // Get result again - should return cached result
        let cached_result = lazy_manager
            .get_interceptor_result("lazy_test")
            .await
            .unwrap();
        assert!(cached_result.is_some());
        assert_eq!(cached_result.unwrap()["interceptor"], "lazy_test");

        // Verify only one execution in state
        let state_read = state.read().await;
        assert_eq!(state_read.pre_request_results.len(), 1);
        assert_eq!(
            state_read.pre_request_results[0].interceptor_name,
            "lazy_test"
        );
    }

    #[tokio::test]
    async fn test_interceptor_factory_and_manager() {
        use crate::routing::InterceptorSpec;
        use std::sync::Arc;

        // Dummy factory that creates TestInterceptor from InterceptorSpec
        struct DummyFactory;
        impl super::InterceptorFactory for DummyFactory {
            fn create_interceptor(
                &self,
                spec: &InterceptorSpec,
            ) -> Result<Arc<dyn super::Interceptor>, super::InterceptorError> {
                Ok(Arc::new(TestInterceptor::new(spec.name.clone())))
            }
        }

        // Example InterceptorSpec
        let spec = InterceptorSpec {
            name: "factory_test_interceptor".to_string(),
            interceptor_type: InterceptorType::Guardrail {
                guard_id: "guard_id".to_string(),
            },
            extra: Default::default(),
        };

        // Use the factory to create an interceptor
        let factory = DummyFactory;
        let interceptor = factory.create_interceptor(&spec).unwrap();

        // Add to manager and execute
        let mut manager = super::InterceptorManager::new();
        manager.add_interceptor(interceptor).unwrap();

        let state = Arc::new(tokio::sync::RwLock::new(super::InterceptorState::new()));
        let request = ChatCompletionRequest::default();
        let headers = HashMap::new();
        let mut context = super::InterceptorContext::new(request, headers, state.clone());

        manager.execute_pre_request(&mut context).await.unwrap();
        let state_read = state.read().await;
        assert_eq!(state_read.pre_request_results.len(), 1);
        assert_eq!(
            state_read.pre_request_results[0].interceptor_name,
            "factory_test_interceptor"
        );
        assert!(state_read.pre_request_results[0].success);
    }
}
