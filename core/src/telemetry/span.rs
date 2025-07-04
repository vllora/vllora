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
            ttft = field::Empty,
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
