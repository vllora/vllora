| Span        | Span Attributes                                                                                                                                         | Events                          | Event Structure                                                                                                             |
|-------------|--------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------|----------------------------------------------------------------------------------------------------------------------------|
| run    |   | RunStarted<br>RunFinished<br>RunError | <ul><li>RunStarted { run_id, thread_id, span_id, parent_span_id, timestamp }</li><li>RunFinished { run_id, thread_id, span_id, parent_span_id, timestamp }</li><li>RunError { run_id, thread_id, span_id, parent_span_id, message, code, timestamp }</li></ul> |
| agent    | - vllora.agent_name  | AgentStarted<br>AgentFinished | <ul><li>AgentStarted { run_id, thread_id, span_id, parent_span_id, timestamp, name }</li><li>AgentFinished { run_id, thread_id, span_id, parent_span_id, timestamp }</li></ul> |
| task    | - vllora.task_name  | TaskStarted<br>TaskFinished | <ul><li>TaskStarted { run_id, thread_id, span_id, parent_span_id, timestamp, name }</li><li>TaskFinished { run_id, thread_id, span_id, parent_span_id, timestamp }</li></ul> |
| {openai, gemini, bedrock, anthropic}    | - tags<br>- retries_left<br>- request<br>- output<br>- error<br>- usage<br>- raw_usage<br>- ttft<br>- message_id<br>- cost  | Custom(LlmStart)<br>TextMessageStart<br>TextMessageContent<br>TextMessageEnd<br>Custom(LlmStop)<br>Custom(Cost) | <ul><li>Custom(LlmStart) { run_id, thread_id, span_id, parent_span_id, timestamp, provider_name, model_name, input }</li><li>TextMessageStart { run_id, thread_id, span_id, parent_span_id, timestamp, role }</li><li>TextMessageContent { run_id, thread_id, span_id, parent_span_id, timestamp, delta }</li><li>TextMessageEnd { run_id, thread_id, span_id, parent_span_id, timestamp }</li><li>Custom(LlmStop) { run_id, thread_id, span_id, parent_span_id, timestamp, content }</li><li>Custom(Cost) { run_id, thread_id, span_id, parent_span_id, timestamp, value }</li></ul> |
| *    |   | Custom(SpanStart)<br>Custom(SpanEnd) | <ul><li>Custom(SpanStart) { run_id, thread_id, span_id, parent_span_id, timestamp, operation_name, attributes }</li><li>Custom(SpanEnd) { run_id, thread_id, span_id, parent_span_id, timestamp, operation_name, attributes, start_time_unix_nano, finish_time_unix_nano }</li></ul> |

## LLM Provider Spans

The `{openai, gemini, bedrock, anthropic}` spans are created at the beginning of each model execution call using the `create_model_span!` macro. These spans encompass the entire lifecycle of a model interaction and serve as the parent context for all events emitted during that interaction.

**Event Flow:**
1. Span is created when model execution begins (via `create_model_span!`)
2. `Custom(LlmStart)` and `TextMessageStart` events are emitted immediately after the request is sent to the provider
3. `TextMessageContent` events are streamed as the model generates tokens (corresponds to internal `LlmContent` events)
4. `TextMessageEnd` and `Custom(LlmStop)` events are emitted when the model completes generation
5. `Custom(Cost)` event is emitted after usage metrics are calculated
6. Span ends when the model execution completes

All events emitted during this flow share the same `span_id` from the parent model span, allowing them to be correlated together. The span attributes (such as `request`, `output`, `usage`, `ttft`) are recorded onto the span as the execution progresses.


