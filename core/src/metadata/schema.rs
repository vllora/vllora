// @generated automatically by Diesel CLI.

diesel::table! {
    knowledge (id) {
        id -> Text,
        name -> Text,
        workflow_id -> Text,
        metadata -> Nullable<Text>,
        description -> Nullable<Text>,
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
        is_custom -> Integer,
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
    provider_credentials (id) {
        id -> Text,
        provider_name -> Text,
        provider_type -> Text,
        credentials -> Text,
        project_id -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        is_active -> Integer,
    }
}

diesel::table! {
    providers (id) {
        id -> Text,
        provider_name -> Text,
        description -> Nullable<Text>,
        endpoint -> Nullable<Text>,
        priority -> Integer,
        privacy_policy_url -> Nullable<Text>,
        terms_of_service_url -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        is_active -> Integer,
        custom_inference_api_type -> Nullable<Text>,
        is_custom -> Integer,
    }
}

diesel::table! {
    metrics (metric_name, timestamp_us, attributes, trace_id, span_id) {
        metric_name -> Text,
        metric_type -> Text,
        value -> Double,
        timestamp_us -> BigInt,
        attributes -> Text,
        project_id -> Nullable<Text>,
        thread_id -> Nullable<Text>,
        run_id -> Nullable<Text>,
        trace_id -> Nullable<Text>,
        span_id -> Nullable<Text>,
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

#[cfg(feature = "sqlite")]
diesel::table! {
    mcp_configs (id) {
        id -> Text,
        company_slug -> Text,
        config -> Text,
        tools -> Text,
        tools_refreshed_at -> Nullable<TimestamptzSqlite>,
        created_at -> TimestamptzSqlite,
        updated_at -> TimestamptzSqlite,
    }
}

#[cfg(feature = "postgres")]
diesel::table! {
    mcp_configs (id) {
        id -> Uuid,
        company_slug -> Text,
        config -> Text,
        tools -> Text,
        tools_refreshed_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    sessions (id) {
        id -> Text,
    }
}

diesel::table! {
    finetune_jobs (id) {
        id -> Text,
        project_id -> Text,
        workflow_id -> Text,
        evaluator_version -> Nullable<Integer>,
        state -> Text,
        provider -> Text,
        provider_job_id -> Text,
        base_model -> Text,
        fine_tuned_model -> Nullable<Text>,
        error_message -> Nullable<Text>,
        training_config -> Nullable<Text>,
        training_file_id -> Nullable<Text>,
        validation_file_id -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        completed_at -> Nullable<Text>,
    }
}

diesel::table! {
    workflows (id) {
        id -> Text,
        name -> Text,
        objective -> Text,
        eval_script -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        deleted_at -> Nullable<Text>,
        state -> Nullable<Text>,
        iteration_state -> Nullable<Text>,
    }
}

diesel::table! {
    workflow_logs (id) {
        id -> Text,
        workflow_id -> Text,
        target -> Nullable<Text>,
        log -> Text,
        created_at -> Text,
    }
}

diesel::table! {
    workflow_records (id) {
        id -> Text,
        workflow_id -> Text,
        data -> Text,
        topic_id -> Nullable<Text>,
        span_id -> Nullable<Text>,
        is_generated -> Integer,
        source_record_id -> Nullable<Text>,
        metadata -> Nullable<Text>,
        created_at -> Text,
    }
}

diesel::table! {
    workflow_record_scores (id) {
        id -> Text,
        record_id -> Text,
        workflow_id -> Text,
        job_id -> Text,
        score_type -> Text,
        score -> Float,
        created_at -> Text,
    }
}

diesel::table! {
    workflow_topics (id) {
        id -> Text,
        reference_id -> Nullable<Text>,
        workflow_id -> Text,
        name -> Text,
        parent_id -> Nullable<Text>,
        system_prompt -> Nullable<Text>,
        created_at -> Text,
    }
}

diesel::table! {
    workflow_topic_sources (id) {
        id -> Text,
        reference_id -> Nullable<Text>,
        workflow_id -> Text,
        topic_id -> Text,
        source_part_id -> Text,
        created_at -> Text,
    }
}

diesel::table! {
    eval_jobs (id) {
        id -> Text,
        workflow_id -> Text,
        cloud_run_id -> Nullable<Text>,
        status -> Text,
        sample_size -> Nullable<Integer>,
        rollout_model -> Nullable<Text>,
        error -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        completed_at -> Nullable<Text>,
        started_at -> Nullable<Text>,
        result -> Nullable<Text>,
    }
}

diesel::table! {
    knowledge_sources (id) {
        id -> Text,
        reference_id -> Nullable<Text>,
        workflow_id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        metadata -> Nullable<Text>,
        created_at -> Text,
        deleted_at -> Nullable<Text>,
    }
}

diesel::table! {
    knowledge_source_parts (id) {
        id -> Text,
        reference_id -> Nullable<Text>,
        source_id -> Text,
        part_type -> Text,
        content -> Text,
        content_metadata -> Nullable<Text>,
        title -> Nullable<Text>,
        extraction_path -> Nullable<Text>,
        extraction_metadata -> Nullable<Text>,
    }
}

diesel::joinable!(provider_credentials -> projects (project_id));
diesel::joinable!(finetune_jobs -> projects (project_id));

diesel::allow_tables_to_appear_in_same_query!(
    eval_jobs,
    finetune_jobs,
    knowledge,
    knowledge_source_parts,
    knowledge_sources,
    metrics,
    models,
    projects,
    provider_credentials,
    providers,
    sessions,
    traces,
    workflow_logs,
    workflow_records,
    workflow_topics,
    workflow_topic_sources,
    workflows,
);
