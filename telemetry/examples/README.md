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

### Step 2: Set Up Context with Baggage

Before creating spans, we establish an OpenTelemetry context with baggage values:

```rust
let context = Context::current().with_baggage(vec![
    KeyValue::new("vllora.run_id", run_id),
    KeyValue::new("vllora.thread_id", thread_id),
]);
```

**Why this matters**: 
- Baggage values are automatically attached to all spans by `BaggageSpanProcessor`
- This enables correlation across different parts of your application
- The context must be set **before** creating spans for baggage to be propagated

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

1. **Create a span** using a macro (e.g., `create_run_span!`)
2. **Record attributes** directly on the span before instrumenting
3. **Instrument function calls** using `.instrument(span)` to associate the span with execution
4. **Nest spans** by creating child spans in the instrumented functions

```rust
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

1. **`.instrument(span)`**: For async function calls
   ```rust
   async fn operation() {
       let span = create_span!(...);
       
       // Record attributes directly on the span
       span.record("key", "value");
       
       // Instrument the function call
       operation_inner().instrument(span).await;
   }
   
   async fn operation_inner() {
       // This code runs within the span's context
       // You can also record attributes here using tracing::Span::current()
   }
   ```

2. **`span.in_scope(|| { ... })`**: For synchronous code
   ```rust
   span.in_scope(|| {
       // Synchronous code runs within span context
   });
   ```

**Why this pattern?**
- Avoids unnecessary async block wrappers
- Clear separation between span setup and execution
- Attributes can be recorded before or during execution
- Easier to read and maintain

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

## Next Steps

- Integrate tracing into your own application
- Configure additional exporters (e.g., Jaeger, Tempo, Datadog)
- Add custom span attributes relevant to your use case
- Set up trace analysis and visualization tools