# LangDB Conditional Routing with Interceptors - Implementation Guide

## Overview

This module implements LangDB's conditional routing engine with interceptors and metadata support. The routing engine enables organizations to control how user requests are handled by AI models based on request conditions, user metadata, and real-time system metrics.

## Current Implementation Status

### ✅ Implemented Features

1. **Basic Conditional Routing**
   - Route conditions with `all`, `any`, and expression operators
   - Support for `eq`, `ne`, `in`, `gt`, `lt`, `gte`, `lte` operators
   - Target specifications with `$any` pools and sorting
   - Message mapping for request/response modification

2. **Interceptor System**
   - Pre-request and post-request interceptors
   - Guardrail integration
   - Rate limiting support

3. **Metrics Integration**
   - Real-time metrics collection
   - Provider and model-level metrics
   - Duration-based metrics (Total, Last15Minutes, LastHour)

## 🚧 TODO: Enhanced Conditional Routing Implementation

### 1. Extra Field Metadata Extraction

**Priority: High**
**Estimated Effort: 1-2 weeks**

#### TODO Items:
- [ ] **Extra Field Integration**
  - [ ] Extract metadata from `Extra.user` field (RequestUser)
  - [ ] Extract metadata from `Extra.variables` field (HashMap<String, serde_json::Value>)
  - [ ] Extract metadata from `Extra.guards` field for guardrail results
  - [ ] Create metadata access patterns for conditional routing

- [ ] **RequestUser Metadata Support**
  - [ ] Add `user.id` extraction for user identification
  - [ ] Add `user.name` extraction for user display
  - [ ] Add `user.email` extraction for user contact
  - [ ] Add `user.tiers` extraction for user tier-based routing

- [ ] **Variables Metadata Support**
  - [ ] Add dynamic variable extraction from `Extra.variables`
  - [ ] Support nested variable access patterns
  - [ ] Add variable validation and type checking
  - [ ] Create variable caching for performance

#### Example Implementation:
```rust
// TODO: Add to conditional/evaluator.rs
pub enum MetadataField {
    // User metadata from Extra.user
    UserId,
    UserName,
    UserEmail,
    UserTiers,
    
    // Dynamic variables from Extra.variables
    Variable(String), // Dynamic access to Extra.variables
    
    // Guardrail results from Extra.guards
    GuardrailResult(String), // Access guardrail results
}
```

### 2. Enhanced Interceptor System

#### TODO Items:
- [ ] **Advanced Guardrails**
  - [ ] Add custom guardrail framework with plugin system

- [ ] **Request/Response Transformation**
  - [ ] Message content modification and enrichment
  - [ ] Header injection and modification
  - [ ] Response redaction and sanitization
  - [ ] Request validation and normalization
  - [ ] Metadata injection and propagation

- [ ] **Interceptor Type Restrictions**
  - [ ] Pre-request interceptors: Allow all interceptor types (guardrails, rate limiters, transformers, etc.)
  - [ ] Post-request interceptors: Allow only guardrails for response validation, message mapper and content filtering

#### Example Implementation:
```rust
// TODO: Add to interceptor/mod.rs
pub enum InterceptorType {
    Guardrail { 
        guard_id: String,
        config: GuardrailConfig 
    },
    SemanticGuardrail { 
        topics: Vec<String>,
        threshold: f64,
        action: GuardrailAction 
    },
    ToxicityGuardrail { 
        threshold: f64,
        action: ToxicityAction,
        categories: Vec<String> 
    },
    ComplianceGuardrail {
        regulations: Vec<String>, // GDPR, HIPAA, etc.
        data_classification: String,
        action: ComplianceAction
    },
    MessageTransformer { 
        rules: Vec<TransformRule>,
        direction: TransformDirection // pre_request, post_response
    },
    MetadataEnricher {
        fields: Vec<String>,
        sources: Vec<MetadataSource>
    },
}

// TODO: Add validation for post-request interceptors
impl InterceptorType {
    pub fn is_allowed_in_post_request(&self) -> bool {
        matches!(self, 
            InterceptorType::Guardrail { .. } |
            InterceptorType::SemanticGuardrail { .. } |
            InterceptorType::ToxicityGuardrail { .. } |
            InterceptorType::ComplianceGuardrail { .. }
        )
    }
}
```

### 3. Rate Limiting System

#### TODO Items:
- [ ] **Rate Limiter Configuration**
  - [ ] Implement manual rate limiter configuration structure
  - [ ] Support different limit targets (InputTokens, OutputTokens, Requests, Cost)
  - [ ] Support different limit entities (UserName, UserId, ProjectId, OrganizationId)
  - [ ] Support different periods (Minute, Hour, Day, Month, Year)

- [ ] **Rate Limiter Engine**
  - [ ] Token bucket algorithm implementation
  - [ ] Sliding window rate limiting
  - [ ] Distributed rate limiting with Redis
  - [ ] Rate limiter state persistence

- [ ] **Rate Limiter Monitoring**
  - [ ] Rate limit usage tracking
  - [ ] Rate limit violation alerts
  - [ ] Rate limit analytics and reporting
  - [ ] Dynamic rate limit adjustment

#### Example Implementation:
```rust
// TODO: Add to interceptor/rate_limiter.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiter {
    pub limit: u64,
    pub limit_target: LimitTarget,
    pub limit_entity: LimitEntity,
    pub period: RateLimitPeriod,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burst_protection: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<RateLimitAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitTarget {
    InputTokens,
    OutputTokens,
    Requests,
    Cost,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitEntity {
    UserName,
    UserId,
    ProjectId,
    OrganizationId,
    Model,
    Provider,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitPeriod {
    Minute,
    Hour,
    Day,
    Month,
    Year,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitAction {
    Block,
    Throttle,
    Redirect(String), // Redirect to different model
    Fallback(String), // Use fallback model
}

impl RateLimiter {
    pub fn new(limit: u64, limit_target: LimitTarget, limit_entity: LimitEntity, period: RateLimitPeriod) -> Self {
        Self {
            limit,
            limit_target,
            limit_entity,
            period,
            burst_protection: None,
            action: None,
        }
    }
}
```

#### Configuration Examples:
```json
{
  "name": "user_token_limit",
  "type": "rate_limiter",
  "limit": 100000,
  "limit_target": "input_tokens",
  "limit_entity": "user_name",
  "period": "day",
  "action": "block"
}
```

```json
{
  "name": "project_cost_limit",
  "type": "rate_limiter",
  "limit": 500,
  "limit_target": "cost",
  "limit_entity": "project_id",
  "period": "month",
  "action": "fallback",
  "fallback_model": "openai/gpt-3.5-turbo"
}
```

```json
{
  "name": "model_request_limit",
  "type": "rate_limiter",
  "limit": 1000,
  "limit_target": "requests",
  "limit_entity": "model",
  "period": "hour",
  "burst_protection": true,
  "action": "throttle"
}
```

### 4. Extra Field Context Management

#### TODO Items:
- [ ] **Extra Field Extraction Engine**
  - [ ] Extract user metadata from `Extra.user`
  - [ ] Extract dynamic variables from `Extra.variables`
  - [ ] Extract guardrail results from `Extra.guards`
  - [ ] Create metadata access patterns

- [ ] **Metadata Caching**
  - [ ] User metadata caching with TTL
  - [ ] Variable metadata caching
  - [ ] Guardrail result caching
  - [ ] Cache invalidation strategies

- [ ] **Metadata Validation**
  - [ ] Required metadata field validation
  - [ ] Metadata format validation
  - [ ] Metadata consistency checks
  - [ ] Metadata security validation

#### Example Implementation:
```rust
// TODO: Add to conditional/metadata.rs
pub struct MetadataManager {
    pub extractors: Vec<Box<dyn MetadataExtractor>>,
    pub cache: MetadataCache,
    pub validators: Vec<Box<dyn MetadataValidator>>,
}

pub trait MetadataExtractor {
    async fn extract(&self, request: &ChatCompletionRequest, extra: &Option<Extra>) -> Result<HashMap<String, serde_json::Value>, RouterError>;
}

pub struct MetadataCache {
    pub user_cache: Cache<String, RequestUser>,
    pub variables_cache: Cache<String, HashMap<String, serde_json::Value>>,
    pub guardrail_cache: Cache<String, GuardrailResult>,
}
```

### 5. Interceptor Context & State Management

#### TODO Items:
- [ ] **Interceptor Context Enhancement**
  - [ ] Rich context with Extra field metadata
  - [ ] Request/response state tracking
  - [ ] Interceptor chain state management
  - [ ] Context serialization for debugging

- [ ] **Interceptor State Persistence**
  - [ ] Rate limiter state persistence
  - [ ] Guardrail state tracking
  - [ ] Interceptor statistics collection
  - [ ] State recovery mechanisms

- [ ] **Interceptor Chain Management**
  - [ ] Conditional interceptor execution
  - [ ] Interceptor dependency management
  - [ ] Interceptor error handling and fallbacks
  - [ ] Interceptor performance monitoring

#### Example Implementation:
```rust
// TODO: Add to interceptor/mod.rs
pub struct InterceptorContext {
    pub request: ChatCompletionRequest,
    pub headers: HashMap<String, String>,
    pub extra: Option<Extra>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub state: InterceptorState,
    pub chain_position: usize,
    pub results: HashMap<String, InterceptorResult>,
}

pub struct InterceptorState {
    pub rate_limiters: HashMap<String, RateLimiterState>,
    pub guardrails: HashMap<String, GuardrailState>,
    pub transformers: HashMap<String, TransformerState>,
    pub statistics: InterceptorStatistics,
}
```

## Testing Strategy

### Unit Tests
- [ ] Extra field metadata extraction tests
- [ ] Interceptor execution tests
- [ ] Conditional routing logic tests
- [ ] Context management tests
- [ ] Rate limiter tests

### Integration Tests
- [ ] End-to-end conditional routing tests
- [ ] Interceptor chain execution tests
- [ ] Metadata caching tests
- [ ] Performance impact tests
- [ ] Rate limiter integration tests

### Load Tests
- [ ] High-throughput conditional routing
- [ ] Concurrent interceptor execution
- [ ] Metadata extraction performance
- [ ] Memory usage under load
- [ ] Rate limiter performance under load

## Configuration Examples

### Basic Tier-Based Routing
```json
{
  "routes": [
    {
      "name": "premium_user_routing",
      "conditions": {
        "all": [
          { "extra.user.tiers": { "in": ["premium", "enterprise"] } },
          { "extra.variables.request_priority": { "eq": "high" } }
        ]
      },
      "targets": {
        "$any": ["openai/gpt-4", "anthropic/claude-3-opus"],
        "sort": { "latency": "MIN" }
      }
    },
    {
      "name": "standard_user_routing",
      "conditions": {
        "extra.user.tiers": { "in": ["standard", "free"] }
      },
      "targets": {
        "$any": ["openai/gpt-3.5-turbo", "anthropic/claude-3-haiku"]
      }
    }
  ]
}
```

### Rate Limiting with Conditional Routing
```json
{
  "pre_request": [
    {
      "name": "user_token_limit",
      "type": "rate_limiter",
      "limit": 100000,
      "limit_target": "input_tokens",
      "limit_entity": "user_name",
      "period": "day"
    }
  ],
  "routes": [
    {
      "name": "within_limit_routing",
      "conditions": {
        "extra.guards.user_token_limit.passed": { "eq": true }
      },
      "targets": {
        "$any": ["openai/gpt-4", "anthropic/claude-3-opus"]
      }
    },
    {
      "name": "limit_exceeded_routing",
      "conditions": {
        "extra.guards.user_token_limit.passed": { "eq": false }
      },
      "targets": {
        "$any": ["openai/gpt-3.5-turbo", "anthropic/claude-3-haiku"]
      }
    }
  ]
}
```

## Performance Considerations

### Optimization Targets
- [ ] Conditional routing decision latency < 5ms
- [ ] Extra field extraction latency < 2ms
- [ ] Interceptor execution latency < 10ms
- [ ] Rate limiter check latency < 1ms
- [ ] Memory usage < 50MB per router

### Monitoring
- [ ] Conditional routing performance metrics
- [ ] Spans storage for later extraction performance
- [ ] Interceptor execution metrics
- [ ] Cache hit rates and efficiency
- [ ] Rate limiter performance metrics

## References

- [Current Router Example](vllora/core/src/routing/router_example.json)
- [Conditional Router Implementation](vllora/core/src/routing/strategy/conditional/)
- [Interceptor Implementation](vllora/core/src/routing/interceptor/)
- [Metrics Implementation](vllora/core/src/routing/strategy/metric.rs)
- [Extra Struct Definition](vllora/core/src/types/gateway.rs)
