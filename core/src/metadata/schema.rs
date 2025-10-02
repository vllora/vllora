// @generated automatically by Diesel CLI.

diesel::table! {
    messages (id) {
        id -> Text,
        model_name -> Nullable<Text>,
        r#type -> Nullable<Text>,
        thread_id -> Nullable<Text>,
        user_id -> Nullable<Text>,
        content_type -> Nullable<Text>,
        content -> Nullable<Text>,
        content_array -> Text,
        tool_call_id -> Nullable<Text>,
        tool_calls -> Nullable<Text>,
        tenant_id -> Nullable<Text>,
        project_id -> Nullable<Text>,
        created_at -> Text,
    }
}

diesel::table! {
    models (id) {
        id -> Nullable<Text>,
        model_name -> Text,
        description -> Nullable<Text>,
        provider_name -> Text,
        model_type -> Text,
        input_token_price -> Nullable<Float>,
        output_token_price -> Nullable<Float>,
        context_size -> Nullable<Integer>,
        capabilities -> Nullable<Text>,
        input_types -> Nullable<Text>,
        output_types -> Nullable<Text>,
        tags -> Nullable<Text>,
        type_prices -> Nullable<Text>,
        mp_price -> Nullable<Float>,
        model_name_in_provider -> Nullable<Text>,
        owner_name -> Text,
        priority -> Integer,
        parameters -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        deleted_at -> Nullable<Text>,
        benchmark_info -> Nullable<Text>,
        cached_input_token_price -> Nullable<Float>,
        cached_input_write_token_price -> Nullable<Float>,
        release_date -> Nullable<Text>,
        langdb_release_date -> Nullable<Text>,
        knowledge_cutoff_date -> Nullable<Text>,
        license -> Nullable<Text>,
        project_id -> Nullable<Text>,
        endpoint -> Nullable<Text>,
    }
}

diesel::table! {
    projects (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        slug -> Text,
        settings -> Nullable<Text>,
        is_default -> Integer,
        archived_at -> Nullable<Text>,
        allowed_user_ids -> Nullable<Text>,
        private_model_prices -> Nullable<Text>,
    }
}

diesel::table! {
    threads (id) {
        id -> Text,
        user_id -> Nullable<Text>,
        title -> Nullable<Text>,
        model_name -> Nullable<Text>,
        created_at -> Text,
        tenant_id -> Nullable<Text>,
        project_id -> Nullable<Text>,
        is_public -> Integer,
        description -> Nullable<Text>,
        keywords -> Text,
    }
}

diesel::table! {
    traces (trace_id, span_id) {
        trace_id -> Text,
        span_id -> Text,
        thread_id -> Nullable<Text>,
        parent_span_id -> Nullable<Text>,
        operation_name -> Text,
        start_time_us -> BigInt,
        finish_time_us -> BigInt,
        attribute -> Text,
        run_id -> Nullable<Text>,
        project_id -> Nullable<Text>,
    }
}

diesel::joinable!(messages -> threads (thread_id));

diesel::allow_tables_to_appear_in_same_query!(messages, models, projects, threads, traces,);
