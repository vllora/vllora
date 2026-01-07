# Tracing Example

This example demonstrates how to use `vllora_telemetry` span macros and OpenTelemetry tracing exporter to instrument your application with distributed tracing.

## Overview

This example shows a complete tracing workflow from initialization to span creation, attribute recording, and trace export. It demonstrates a hierarchical span structure representing a typical AI agent workflow.

## How Tracing Works - Step by Step

### Step 1: Initialize Tracing Infrastructure

The tracing system is initialized in the `init_tracing()` function:

1. **Create Exporters**: 
   - **OTLP Exporter**: Sends traces to an OpenTelemetry Protocol (OTLP) endpoint (e.g., OpenTelemetry Collector, Jaeger, Tempo)

2. **Create Tracer Provider**:
   - Sets up the `SdkTracerProvider` with:
     - **BaggageSpanProcessor**: Automatically propagates baggage values (like `vllora.run_id`, `vllora.thread_id`) to all spans
     - **OTLP Exporter**: Sends traces to the OTLP endpoint
     - **ID Generator**: Uses UUID-based trace and span IDs

3. **Set Up Tracing Subscriber**:
   - **OpenTelemetry Layer**: Bridges `tracing` crate spans to OpenTelemetry spans
   - **Fmt Layer**: Provides formatted logging with `RUST_LOG` support
   - **EnvFilter**: Filters logs based on `RUST_LOG` environment variable

```rust
// The tracer provider manages span lifecycle and export
let trace_provider = SdkTracerProvider::builder()
    .with_span_processor(BaggageSpanProcessor::new([...]))
    .with_batch_exporter(otlp_exporter)
    .build();
```

### Combining vLLora Traces with Custom App Traces

This example demonstrates how to combine vLLora's built-in tracing with your own custom application traces. This is achieved by using **multiple OpenTelemetry layers** with different target filters:

```rust
// Create separate layers for vLLora traces and custom app traces
let vllora_layer = events::layer("vllora::user_tracing", LevelFilter::INFO, tracer.clone());
let custom_layer = events::layer("app::runtime", LevelFilter::INFO, tracer);

// Register both layers with the subscriber
Registry::default()
    .with(env_filter)
    .with(fmt_layer)
    .with(vllora_layer)    // Captures vLLora spans (target: "vllora::user_tracing")
    .with(custom_layer)    // Captures custom app spans (target: "app::runtime")
    .try_init()
    .expect("Failed to initialize tracing subscriber");
```

**How it works**:
- **Target filtering**: Each layer only processes spans that match its target prefix
- **vLLora layer**: Processes spans created by vLLora macros (e.g., `create_run_span!`, `create_agent_span!`, etc.) which have target `"vllora::user_tracing::*"`
- **Custom layer**: Processes your application's custom spans with target `"app::runtime::*"`
- **Shared tracer**: Both layers use the same tracer, so all spans are part of the same trace and share the same trace ID

**Creating custom app spans**:

```rust
async fn execute_flow() {
    // Create a custom span with target "app::runtime::execution"
    let span = tracing::info_span!(
        target: "app::runtime::execution", 
        "execution",
        execution_id = 123,
    );

    // Instrument async functions using .instrument(span)
    run_operation().instrument(span).await;
}
```

**Key points**:
1. **Target prefix**: Custom spans must use a target that starts with `"app::runtime"` to be captured by the custom layer
2. **Async instrumentation**: Use `.instrument(span)` for async functions (not `span.in_scope()` which only works for sync code)
3. **Span hierarchy**: Your custom spans can be parents or children of vLLora spans - they'll all be part of the same trace
4. **Baggage propagation**: Baggage values set in the context are automatically propagated to both vLLora and custom spans via `BaggageSpanProcessor`

**Example trace structure**:
```
execution (custom app span, target: "app::runtime::execution")
└── run (vLLora span, target: "vllora::user_tracing::run")
    └── agent (vLLora span, target: "vllora::user_tracing::agent")
        └── task (vLLora span, target: "vllora::user_tracing::task")
            └── thread (vLLora span, target: "vllora::user_tracing::api_invoke")
                └── model_invoke (vLLora span, target: "vllora::user_tracing::models")
                    └── openai (vLLora span, target: "vllora::user_tracing::openai")
                        └── tool (vLLora span, target: "vllora::user_tracing::tool")
```

### Step 2: Set Up Context with Baggage

Before creating spans, we establish an OpenTelemetry context with baggage values:

```rust
let context = Context::current().with_baggage(vec![
    KeyValue::new("vllora.run_id", run_id),
    KeyValue::new("vllora.thread_id", thread_id),
    KeyValue::new("vllora.tenant", "default"),
]);
```

**Why this matters**: 
- Baggage values are automatically attached to all spans by `BaggageSpanProcessor`
- This enables correlation across different parts of your application
- The context must be set **before** creating spans for baggage to be propagated
- Baggage values propagate to both vLLora spans and custom app spans

### Step 3: Create Hierarchical Spans

The example creates a nested span hierarchy representing an AI agent workflow:

```
run (top-level execution)
└── agent (agent instance)
    └── task (specific task)
        └── thread (conversation thread)
            └── model_invoke (LLM invocation)
                └── openai (provider-specific call)
                    └── tool (tool execution)
```

**How span hierarchy works**:

1. **Create a span** using a macro (e.g., `create_run_span!`) or `tracing::info_span!` for custom spans
2. **Record attributes** directly on the span before instrumenting
3. **Instrument function calls** using `.instrument(span)` to associate the span with execution
4. **Nest spans** by creating child spans in the instrumented functions

```rust
// Custom app span
async fn execute_flow() {
    let span = tracing::info_span!(
        target: "app::runtime::execution", 
        "execution",
        execution_id = 123,
    );
    
    // Instrument async function calls
    run_operation().instrument(span).await;
}

// vLLora span
async fn run_operation() {
    let run_span = create_run_span!({...});
    
    tracing::info!("1. Created run span");
    // Record attributes directly on the span
    run_operation_inner()
        .instrument(run_span)  // Associates span with this function call
        .await;
}

async fn run_operation_inner() {
    // This code runs within the run_span context
    agent_operation().await; // Creates child span
}
```

**Key concepts**:
- **Span**: Represents a unit of work with a start and end time
- **Parent-Child Relationship**: Child spans are nested within parent spans
- **Trace**: All spans with the same `trace_id` form a trace
- **Context Propagation**: The current span context is automatically propagated to child spans

### Step 4: Record Span Attributes

Attributes are key-value pairs that provide context about what happened during the span. You can record them directly on the span before instrumenting, or within the instrumented function using `tracing::Span::current()`:

```rust
// Record attributes directly on the span (before instrumenting)
let model_invoke_span = create_model_invoke_span!(...);
model_invoke_span.record("input", "What is the capital of France?");
model_invoke_span.record("output", "The capital of France is Paris.");
model_invoke_span.record("cost", serde_json::to_string(&cost_json).unwrap());
model_invoke_span.record("ttft", 150u64); // Time to first token

// Or record within instrumented function
async fn operation_inner() {
    let current_span = tracing::Span::current();
    current_span.record("ttft", 843129u64); // Recorded after async work
}
```

**Common attributes in this example**:
- **Model Invoke Span**: `input`, `output`, `usage`, `cost`, `ttft`, `model`, `provider_name`
- **OpenAI Span**: `request`, `input`, `retries_left`, `ttft`, `usage`, `raw_usage`, `cost`, `output`
- **Tool Span**: `input`, `output`, `tool_name`

### Step 5: Span Lifecycle

Each span goes through these stages:

1. **Creation**: Span is created with a name and optional initial attributes
2. **Attribute Recording**: Attributes can be recorded directly on the span before instrumenting
3. **Active**: Code executes within the span's context (via `.instrument()`)
4. **Additional Recording**: More attributes can be recorded during execution using `tracing::Span::current()`
5. **Completion**: When the instrumented function completes, the span ends
6. **Export**: Completed spans are batched and sent to exporters

**Timing**:
- `start_time`: When the span is created
- `end_time`: When the instrumented block completes
- `duration`: Automatically calculated as `end_time - start_time`

### Step 6: Trace Export

Traces are exported via the **OTLP Exporter**:
- Batches spans and sends them via gRPC to the OTLP endpoint
- Uses the OpenTelemetry Protocol for interoperability
- Spans are sent asynchronously in batches for efficiency

**Export timing**:
- Spans are exported when:
  - A batch is full (configurable)
  - A timeout expires (configurable)
  - The tracer provider is dropped (flushes remaining spans)

```rust
// Wait for traces to be exported
tokio::time::sleep(Duration::from_secs(10)).await;

// Drop tracer provider to flush remaining spans
drop(global::tracer_provider());
```

## Span Hierarchy Details

### 1. Run Span (`run_operation`)
- **Purpose**: Top-level execution context
- **Attributes**: Environment tags
- **Macro**: `create_run_span!`

### 2. Agent Span (`agent_operation`)
- **Purpose**: Represents an AI agent instance
- **Attributes**: Agent name
- **Macro**: `create_agent_span!`

### 3. Task Span (`task_operation`)
- **Purpose**: Represents a specific task or goal
- **Attributes**: Task name
- **Macro**: `create_task_span!`

### 4. Thread Span (`thread_operation`)
- **Purpose**: Represents a conversation thread
- **Attributes**: `user_id`, `session_id`
- **Macro**: `create_thread_span!`

### 5. Model Invoke Span (`model_invoke_operation`)
- **Purpose**: Represents an LLM invocation
- **Attributes**: `input`, `output`, `usage`, `cost`, `ttft`, `model`, `provider_name`, `inference_model_name`, `credentials_identifier`
- **Macro**: `create_model_invoke_span!`

### 6. OpenAI Span (`openai_operation`)
- **Purpose**: Provider-specific implementation details
- **Attributes**: `request`, `input`, `retries_left`, `ttft`, `usage`, `raw_usage`, `cost`, `output`
- **Macro**: `create_model_span!` with `SPAN_OPENAI`

### 7. Tool Span (`tool_operation`)
- **Purpose**: Represents tool/function execution
- **Attributes**: `input`, `output`, `tool_name`
- **Macro**: `create_tool_span!`

## Running the Example

### Basic Usage

```bash
cargo run --example tracing_example -- --otlp-endpoint http://localhost:4317
```

### With Verbose Logging

```bash
RUST_LOG=debug cargo run --example tracing_example -- --otlp-endpoint http://localhost:4317 --verbose
```

### Custom OTLP Endpoint

```bash
cargo run --example tracing_example -- --otlp-endpoint http://your-otel-collector:4317
```

## Prerequisites

- An OTLP-compatible collector or backend (e.g., OpenTelemetry Collector, Jaeger, Tempo, etc.)
- The collector should be running and listening on the specified endpoint

## Understanding the Output

### OTLP Export

Traces sent to the OTLP endpoint include:
- **Trace ID**: Unique identifier for the entire trace
- **Span ID**: Unique identifier for each span
- **Parent Span ID**: Links child spans to their parents
- **Timestamps**: Start and end times for each span
- **Attributes**: All recorded key-value pairs
- **Status**: Success, error, or unset

## Key Concepts

### Context Propagation

The OpenTelemetry context carries:
- **Current Span**: The active span in the current execution context
- **Baggage**: Key-value pairs that propagate across service boundaries
- **Trace State**: Additional trace metadata

### Span Instrumentation

The recommended pattern is to directly instrument function calls:

1. **`.instrument(span)`**: For async function calls (recommended for async code)
   ```rust
   async fn operation() {
       let span = tracing::info_span!(
           target: "app::runtime::operation",
           "operation"
       );
       
       // Record attributes directly on the span
       span.record("key", "value");
       
       // Instrument the async function call
       operation_inner().instrument(span).await;
   }
   
   async fn operation_inner() {
       // This code runs within the span's context
       // You can also record attributes here using tracing::Span::current()
       let current_span = tracing::Span::current();
       current_span.record("another_key", "another_value");
   }
   ```

2. **`span.enter()`**: Alternative for async code (returns a guard)
   ```rust
   async fn operation() {
       let span = tracing::info_span!(...);
       
       // Enter the span - guard keeps it active across await points
       let _span_guard = span.enter();
       
       // All code here runs within the span context
       await some_async_operation();
       await another_async_operation();
       
       // Guard is dropped here, span exits
   }
   ```

3. **`span.in_scope(|| { ... })`**: For synchronous code only
   ```rust
   span.in_scope(|| {
       // Synchronous code runs within span context
       // NOTE: Cannot use await inside this closure!
   });
   ```

**Why use `.instrument(span)` for async functions?**
- Works seamlessly with async/await
- Clear separation between span setup and execution
- Attributes can be recorded before or during execution
- Easier to read and maintain
- Automatically handles span lifecycle across await points

**When to use `span.enter()` vs `.instrument()`:**
- Use `.instrument(span)` when you want to instrument a specific async function call
- Use `span.enter()` when you want to keep a span active across multiple async operations in the same function

### Attribute Recording

Attributes can be recorded in several ways:

1. **At span creation**: Via macro parameters (built into the span)
2. **Before instrumenting**: Directly on the span object
   ```rust
   span.record("key", "value");
   operation_inner().instrument(span).await;
   ```
3. **During execution**: Using `tracing::Span::current().record()` within instrumented functions
   ```rust
   async fn operation_inner() {
       let current_span = tracing::Span::current();
       current_span.record("key", "value");
   }
   ```
4. **As JSON**: Complex data structures can be serialized to JSON strings
   ```rust
   span.record("cost", serde_json::to_string(&cost_json).unwrap());
   ```

## Best Practices

1. **Set context before creating spans**: Baggage values won't propagate if context is set after span creation
2. **Use appropriate span hierarchy**: Reflect your application's logical structure
3. **Record meaningful attributes**: Include data that helps with debugging and analysis
4. **Keep spans focused**: Each span should represent a single logical operation
5. **Handle errors**: Record error information in span attributes or status

## Troubleshooting

### Spans not appearing in collector

- Check that the OTLP endpoint is correct and accessible
- Ensure the collector is running and listening on the specified port
- Wait for the batch export timeout (spans are exported in batches)
- Check collector logs for errors

### Baggage values not appearing

- Ensure context is set **before** creating spans
- Verify `BaggageSpanProcessor` is configured with the correct baggage keys
- Check that `.with_context(context)` is called on the outermost async block

### Custom spans not appearing in traces

- Verify the span target matches the layer filter (e.g., `"app::runtime::*"` for custom layer)
- Ensure the custom layer is registered with the subscriber
- Check that the span is created before being instrumented
- Verify you're using `.instrument(span)` for async functions, not `span.in_scope()`

## Summary: Combining vLLora and Custom Traces

This example demonstrates a complete setup for combining vLLora's built-in tracing with your custom application traces:

1. **Dual Layer Setup**: Use separate OpenTelemetry layers for vLLora traces (`vllora::user_tracing`) and your own application's traces (e.g., `my_app::backend` or another prefix you choose; `app::runtime` is just an example)

2. **Shared Context**: All spans share the same trace ID and baggage values, enabling end-to-end trace correlation

3. **Async Instrumentation**: Use `.instrument(span)` for async functions to properly handle span lifecycle across await points

4. **Hierarchical Structure**: Custom app spans can wrap vLLora spans, creating a unified trace hierarchy

5. **Baggage Propagation**: Context baggage (run_id, thread_id, tenant, etc.) automatically propagates to all spans via `BaggageSpanProcessor`

**Quick Reference**:
- **vLLora spans**: Use macros like `create_run_span!`, `create_agent_span!`, etc. (target: `vllora::user_tracing::*`)
- **Custom spans**: Use `tracing::info_span!` with target `app::runtime::*`
- **Async instrumentation**: `function().instrument(span).await`
- **Sync instrumentation**: `span.in_scope(|| { ... })`

## Next Steps

- Integrate tracing into your own application
- Configure additional exporters (e.g., Jaeger, Tempo, Datadog)
- Add custom span attributes relevant to your use case
- Set up trace analysis and visualization tools
- Experiment with different span hierarchies to match your application structure