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
            error = field::Empty,
            usage = field::Empty,
            raw_usage = field::Empty,
            ttft = field::Empty,
            message_id = field::Empty,
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
macro_rules! create_thread_span {
    ($tags:expr) => {{
        tracing::info_span!(
            target: "langdb::user_tracing::api_invoke",
            SPAN_API_INVOKE,
            request = tracing::field::Empty,
            response = tracing::field::Empty,
            error = tracing::field::Empty,
            thread_id = tracing::field::Empty,
            message_id = tracing::field::Empty,
            cost = tracing::field::Empty,
            credentials_identifier = tracing::field::Empty,
            router_name = tracing::field::Empty,
            tags = JsonValue(&serde_json::to_value($tags.clone()).unwrap_or_default()).as_value(),
            user = tracing::field::Empty,
        )
    }};
}
