#[macro_export]
macro_rules! create_model_span {
    // Variant with span name, tags, level and custom fields
    ($name:expr, $target:expr, $tags:expr, $retries_left:expr, $($field_name:ident = $field_value:expr),* $(,)?) => {{
        static NAME: &str = $name; // Capture the name expression
        tracing::info_span!(
            target: $target,
            NAME, // Use string interpolation for the span name
            tags = JsonValue(&serde_json::to_value($tags.clone()).unwrap_or_default()).as_value(),
            retries_left = $retries_left,
            request = field::Empty,
            output = field::Empty,
            error_payload = field::Empty,
            error = field::Empty,
            usage = field::Empty,
            raw_usage = field::Empty,
            ttft = field::Empty,
            cost = field::Empty,
            $($field_name = $field_value,)*
        )
    }};

    // Variant with span name, target, tags, retries and custom fields
    ($name:expr, $target:expr, $tags:expr, $retries_left:expr) => {{
        $crate::create_model_span!($name, $target, $tags, $retries_left,)
    }};

    // Variant with span name, target, tags and custom fields
    ($name:expr, $target:expr, $tags:expr) => {{
        $crate::create_model_span!($name, $target, $tags, 0,)
    }};
}

#[macro_export]
macro_rules! create_api_invoke_span {
    ($tags:expr) => {{
        tracing::info_span!(
            target: "vllora::user_tracing::api_invoke",
            $crate::events::SPAN_API_INVOKE,
            request = tracing::field::Empty,
            response = tracing::field::Empty,
            error = tracing::field::Empty,
            thread_id = tracing::field::Empty,
            cost = tracing::field::Empty,
            credentials_identifier = tracing::field::Empty,
            router_name = tracing::field::Empty,
            tags = JsonValue(&serde_json::to_value($tags.clone()).unwrap_or_default()).as_value(),
            user = tracing::field::Empty,
            usage = tracing::field::Empty,
        )
    }};
}

#[macro_export]
macro_rules! create_run_span {
    ($tags:expr) => {{
        tracing::info_span!(
            target: "vllora::user_tracing::run",
            "run",
        )
    }};
}

#[macro_export]
macro_rules! create_agent_span {
    ($agent_name:expr) => {{
        tracing::info_span!(
            target: "vllora::user_tracing::agent",
            "agent",
            "vllora.agent_name" = $agent_name,
        )
    }};
}

#[macro_export]
macro_rules! create_task_span {
    ($task_name:expr) => {{
        tracing::info_span!(
            target: "vllora::user_tracing::task",
            "task",
            "vllora.task_name" = $task_name,
        )
    }};
}

#[macro_export]
macro_rules! create_tool_span {
    ($tool_name:expr, $tags:expr) => {{
        tracing::info_span!(
            target: "vllora::user_tracing::tool",
            "tool",
            "vllora.tool_name" = $tool_name,
            tags = JsonValue(&serde_json::to_value($tags.clone()).unwrap_or_default()).as_value(),
        )
    }};

    ($tool_name:expr) => {{
        tracing::info_span!(
            target: "vllora::user_tracing::tool",
            "tool",
            "vllora.tool_name" = $tool_name,
        )
    }};
}

#[macro_export]
macro_rules! create_model_invoke_span {
    // Variant with parent span
    ($input:expr, $model:expr, $provider_name:expr, $model_name:expr, $inference_model_name:expr, $credentials_identifier:expr, $tags:expr, parent = $parent:expr) => {{
        tracing::info_span!(
            target: "vllora::user_tracing::models",
            parent: $parent,
            $crate::events::SPAN_MODEL_CALL,
            input = $input,
            model = $model,
            provider_name = $provider_name,
            model_name = $model_name,
            inference_model_name = $inference_model_name,
            credentials_identifier = $credentials_identifier,
            output = tracing::field::Empty,
            error = tracing::field::Empty,
            cost = tracing::field::Empty,
            usage = tracing::field::Empty,
            ttft = tracing::field::Empty,
            tags = $crate::events::JsonValue(&serde_json::to_value($tags.clone()).unwrap_or_default()).as_value(),
            cache = tracing::field::Empty,
        )
    }};

    // Variant without parent span
    ($input:expr, $model:expr, $provider_name:expr, $model_name:expr, $inference_model_name:expr, $credentials_identifier:expr, $tags:expr) => {{
        tracing::info_span!(
            target: "vllora::user_tracing::models",
            $crate::events::SPAN_MODEL_CALL,
            input = $input,
            model = $model,
            provider_name = $provider_name,
            model_name = $model_name,
            inference_model_name = $inference_model_name,
            credentials_identifier = $credentials_identifier,
            output = tracing::field::Empty,
            error = tracing::field::Empty,
            cost = tracing::field::Empty,
            usage = tracing::field::Empty,
            ttft = tracing::field::Empty,
            tags = $crate::events::JsonValue(&serde_json::to_value($tags.clone()).unwrap_or_default()).as_value(),
            cache = tracing::field::Empty,
        )
    }};

    // Variant without parent span and tags
    ($input:expr, $model:expr, $provider_name:expr, $model_name:expr, $inference_model_name:expr, $credentials_identifier:expr) => {{
        tracing::info_span!(
            target: "vllora::user_tracing::models",
            $crate::events::SPAN_MODEL_CALL,
            input = $input,
            model = $model,
            provider_name = $provider_name,
            model_name = $model_name,
            inference_model_name = $inference_model_name,
            credentials_identifier = $credentials_identifier,
            output = tracing::field::Empty,
            error = tracing::field::Empty,
            cost = tracing::field::Empty,
            usage = tracing::field::Empty,
            ttft = tracing::field::Empty,
            tags = tracing::field::Empty,
            cache = tracing::field::Empty,
        )
    }};
}
