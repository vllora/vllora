use crate::credentials::GatewayCredentials;
use crate::error::GatewayError;
use crate::executor::chat_completion::basic_executor::BasicCacheContext;
use crate::executor::chat_completion::stream_executor::{stream_chunks, StreamCacheContext};
use crate::handler::ModelEventWithDetails;
use crate::mcp::McpConfig;
use crate::model::cached::CachedModel;
use crate::model::tools::GatewayTool;
use crate::model::ResponseCacheState;
use crate::GatewayApiError;
use vllora_llm::client::completions::response_stream::ResultStream;
use vllora_llm::client::message_mapper::MessageMapper;
use vllora_llm::error::LLMError;
use vllora_llm::mcp::get_tools;
use vllora_llm::types::credentials::Credentials;
use vllora_llm::types::engine::{
    CompletionEngineParamsBuilder, CompletionModelDefinition, CompletionModelParams,
    ExecutionOptions, Model,
};
use vllora_llm::types::gateway::{
    ChatCompletionMessage, ChatCompletionRequestWithTools, ChatCompletionResponse, Extra,
};
use vllora_llm::types::instance::ModelInstance;
use vllora_llm::types::models::ModelMetadata;
use vllora_llm::types::models::ModelType;
use vllora_llm::types::provider::InferenceModelProvider;
use vllora_llm::types::tools::ModelTool;
use vllora_llm::types::tools::ModelTools;
use vllora_llm::types::tools::Tool;
use vllora_llm::types::ModelEvent;
use vllora_llm::types::ModelEventType;

use either::Either::{self, Left, Right};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::Span;
use tracing_futures::Instrument;
use uuid::Uuid;
use vllora_llm::types::credentials_ident::CredentialsIdent;

use super::context::ExecutorContext;
use crate::executor::chat_completion::breakpoint::{wait_for_breakpoint_action, BreakpointManager};

pub mod basic_executor;
pub mod breakpoint;
pub mod routed_executor;
pub mod stream_executor;
pub mod stream_wrapper;

pub type ChatCompletionExecutionResult =
    Either<Result<ResultStream, GatewayApiError>, Result<ChatCompletionResponse, GatewayApiError>>;

#[tracing::instrument(level = "debug", skip_all)]
pub async fn execute<T: Serialize + DeserializeOwned + Debug + Clone>(
    request_with_tools: &ChatCompletionRequestWithTools<T>,
    executor_context: &ExecutorContext,
    router_span: tracing::Span,
    stream_cache_context: StreamCacheContext,
    basic_cache_context: BasicCacheContext,
    llm_model: &ModelMetadata,
    breakpoint_manager: Option<&BreakpointManager>,
    thread_id: Option<&String>,
) -> Result<ChatCompletionExecutionResult, GatewayApiError> {
    let span = Span::current();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(1000);

    let (tools, tools_map) =
        resolve_mcp_tools(executor_context.mcp_config.as_ref(), request_with_tools).await?;

    let mut cached_instance = None;
    let mut cache_state = match request_with_tools.extra {
        Some(Extra { cache: Some(_), .. }) => Some(ResponseCacheState::Miss),
        _ => None,
    };
    if request_with_tools.request.stream.unwrap_or(false) {
        if let Some(events) = &stream_cache_context.cached_events {
            cached_instance = Some(CachedModel::new(events.clone(), None));
            cache_state = Some(ResponseCacheState::Hit);
        }
    } else if let Some(events) = &basic_cache_context.cached_events {
        if let Some(response) = &basic_cache_context.cached_response {
            cached_instance = Some(CachedModel::new(events.clone(), Some(response.clone())));
            cache_state = Some(ResponseCacheState::Hit);
        }
    }

    let mut request_to_use = request_with_tools.request.clone();
    if let Some(breakpoint_manager) = breakpoint_manager {
        match wait_for_breakpoint_action(
            &executor_context.tags,
            breakpoint_manager,
            &request_with_tools.request,
            &executor_context.callbackhandler,
            thread_id,
        )
        .await
        {
            Ok(modified_request) => {
                request_to_use = modified_request;
            }
            Err(e) => {
                tracing::error!("Breakpoint error: {}", e);
                return Err(GatewayApiError::CustomError(format!(
                    "Breakpoint error: {}",
                    e
                )));
            }
        }
    }

    let key = GatewayCredentials::extract_key_from_model(
        llm_model,
        &executor_context.project_id.to_string(),
        "default",
        executor_context.key_storage.as_ref().as_ref(),
    )
    .await
    .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;
    // Create a modified request_with_tools with the potentially modified request
    let mut modified_request_with_tools = request_with_tools.clone();
    modified_request_with_tools.request = request_to_use.clone();

    let resolved_model_context = resolve_model_instance(
        executor_context,
        &modified_request_with_tools,
        tools_map,
        tools,
        router_span,
        request_with_tools.extra.as_ref(),
        request_to_use.messages.clone(),
        cached_instance,
        cache_state,
        llm_model,
        key.as_ref(),
    )
    .await?;

    let mut request = request_to_use;
    request.model = llm_model.inference_provider.model_name.clone();

    let user: String = request
        .user
        .as_ref()
        .map_or(Uuid::new_v4().to_string(), |v| v.clone());

    let mut messages = vec![];

    for message in &request.messages {
        messages.push(
            MessageMapper::map_completions_message_to_vllora_message(
                message,
                &request.model,
                &user.to_string(),
            )
            .map_err(LLMError::from)?,
        );
    }
    let ch = executor_context.callbackhandler.clone();
    let db_model = resolved_model_context.db_model.clone();
    let handle = tokio::spawn(async move {
        let mut stop_event = None;
        let mut tool_calls = None;
        while let Some(Some(msg)) = rx.recv().await {
            if let ModelEvent {
                event: ModelEventType::LlmStop(e),
                ..
            } = &msg
            {
                stop_event = Some(e.clone());
            }

            if let ModelEvent {
                event: ModelEventType::ToolStart(e),
                ..
            } = &msg
            {
                if tool_calls.is_none() {
                    tool_calls = Some(vec![]);
                }
                tool_calls.as_mut().unwrap().push(e.clone());
            }

            ch.on_message(ModelEventWithDetails::new(msg, Some(db_model.clone())));
        }

        (stop_event, tool_calls)
    });

    let is_stream = request.stream.unwrap_or(false);
    if is_stream {
        // if let Some(Extra { guards, .. }) = &request_with_tools.extra {
        //     if !guardrails.is_empty() {
        //         for guardrail in guardrails {
        //             let guard_stage = match guardrail {
        //                 GuardOrName::Guard(guard) => guard.stage(),
        //                 GuardOrName::GuardWithParameters(GuardWithParameters { id, .. }) => {
        //                     executor_context
        //                         .guards
        //                         .as_ref()
        //                         .and_then(|guards| guards.get(id))
        //                         .ok_or_else(|| {
        //                             GatewayApiError::GuardError(GuardError::GuardNotFound(
        //                                 id.clone(),
        //                             ))
        //                         })?
        //                         .stage()
        //                 }
        //             };

        //             if guard_stage == &GuardStage::Output {
        //                 return Err(GatewayApiError::GuardError(
        //                     GuardError::OutputGuardrailsNotSupportedInStreaming,
        //                 ));
        //             }
        //         }
        //     }
        // }
    }

    let input_vars = request_with_tools
        .extra
        .as_ref()
        .and_then(|e| e.variables.clone())
        .unwrap_or_default();
    if is_stream {
        Ok(Left(
            stream_chunks(
                resolved_model_context.completion_model_definition,
                resolved_model_context.model_instance,
                messages.clone(),
                executor_context.callbackhandler.clone().into(),
                executor_context.tags.clone(),
                input_vars,
                stream_cache_context,
            )
            .instrument(span)
            .await,
        ))
    } else {
        let result = basic_executor::execute(
            request,
            resolved_model_context.model_instance,
            messages.clone(),
            executor_context.tags.clone(),
            tx,
            span.clone(),
            Some(handle),
            input_vars,
            basic_cache_context,
            Some(resolved_model_context.db_model.clone()),
        )
        .instrument(span)
        .await;

        // if let Ok(completion_response) = &result {
        //     let ChatCompletionResponse { choices, .. } = completion_response;
        //     for choice in choices {
        //         apply_guardrails(
        //             &[choice.message.clone()],
        //             request_with_tools.extra.as_ref(),
        //             executor_context.evaluator_service.as_ref().as_ref(),
        //             executor_context,
        //             GuardStage::Output,
        //         )
        //         .await?;
        //     }
        // }

        Ok(Right(result))
    }
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn resolve_model_instance<T: Serialize + DeserializeOwned + Debug + Clone>(
    executor_context: &ExecutorContext,
    request: &ChatCompletionRequestWithTools<T>,
    tools_map: HashMap<String, Arc<Box<dyn Tool>>>,
    tools: ModelTools,
    router_span: Span,
    extra: Option<&Extra>,
    initial_messages: Vec<ChatCompletionMessage>,
    cached_model: Option<CachedModel>,
    cache_state: Option<ResponseCacheState>,
    llm_model: &ModelMetadata,
    key: Option<&Credentials>,
) -> Result<ResolvedModelContext, GatewayApiError> {
    let provider_specific = request.provider_specific.clone();
    let execution_options = request
        .max_retries
        .map(|retries| ExecutionOptions {
            max_retries: Some(retries),
        })
        .unwrap_or_default();

    let request = request.request.clone();

    let mut builder =
        CompletionEngineParamsBuilder::new().with_provider(llm_model.inference_provider.clone());

    builder = builder.with_model_name(llm_model.inference_provider.model_name.clone());

    if let Some(credentials) = key {
        builder = builder.with_credentials(credentials.clone());
    }

    if let Some(provider_specific) = provider_specific {
        builder = builder.with_provider_specific(provider_specific.clone());
    }

    if let Some(execution_options) = Some(execution_options.clone()) {
        builder = builder.with_execution_options(execution_options.clone());
    }

    let engine = builder.build(&request)?;

    let credentials_ident = if llm_model.inference_provider.provider
        == InferenceModelProvider::Proxy("vllora".to_string())
    {
        CredentialsIdent::Vllora
    } else {
        CredentialsIdent::Own
    };

    let db_model = Model {
        name: llm_model.model.clone(),
        inference_model_name: llm_model.inference_provider.model_name.clone(),
        provider_name: llm_model.inference_provider.provider.to_string(),
        model_type: ModelType::Completions,
        price: llm_model.price.clone(),
        credentials_ident,
    };

    let completion_model_definition = CompletionModelDefinition {
        name: format!(
            "{}/{}",
            llm_model.inference_provider.provider, llm_model.model
        ),
        model_params: CompletionModelParams {
            engine: engine.clone(),
            provider_name: llm_model.model_provider.to_string(),
            prompt_name: None,
        },
        tools,
        db_model: db_model.clone(),
    };

    let model_instance = crate::model::init_completion_model_instance(
        completion_model_definition.clone(),
        tools_map,
        executor_context,
        router_span.clone(),
        extra,
        initial_messages,
        cached_model,
        cache_state,
        request.clone(),
    )
    .await
    .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    Ok(ResolvedModelContext {
        completion_model_definition,
        model_instance,
        db_model,
        llm_model: llm_model.clone(),
    })
}

pub async fn resolve_mcp_tools<T: Serialize + DeserializeOwned + Debug + Clone>(
    mcp_config: Option<&McpConfig>,
    request: &ChatCompletionRequestWithTools<T>,
) -> Result<(ModelTools, HashMap<String, Arc<Box<dyn Tool>>>), GatewayApiError> {
    let mut request_tools = vec![];
    let mut tools_map = HashMap::new();
    if let Some(tools) = &request.request.tools {
        for tool in tools {
            request_tools.push(ModelTool {
                name: tool.function.name.clone(),
                description: tool.function.description.clone(),
                passed_args: vec![],
            });

            tools_map.insert(
                tool.function.name.clone(),
                Arc::new(Box::new(GatewayTool { def: tool.clone() }) as Box<dyn Tool>),
            );
        }
    }

    let mcp_tools = match &request.mcp_servers {
        Some(tools) => get_tools(tools)
            .await
            .map_err(|e| GatewayError::McpServerError(Box::new(e)))?,
        None => Vec::new(),
    };

    for server_tools in mcp_tools {
        for tool in server_tools.tools {
            tools_map.insert(
                tool.name(),
                Arc::new(Box::new(tool.clone()) as Box<dyn Tool>),
            );
            request_tools.push(tool.into());
        }
    }

    if let Some(mcp_config) = mcp_config {
        for (_name, config) in mcp_config.mcp_servers.iter() {
            let definition = config.to_mcp_definition();
            let tools = get_tools(&[definition])
                .await
                .map_err(|e| GatewayError::McpServerError(Box::new(e)))?;
            for server_tools in tools {
                for tool in server_tools.tools {
                    tools_map.insert(
                        tool.name(),
                        Arc::new(Box::new(tool.clone()) as Box<dyn Tool>),
                    );
                    request_tools.push(tool.into());
                }
            }
        }
    }

    Ok((ModelTools(request_tools), tools_map))
}

pub struct ResolvedModelContext {
    pub completion_model_definition: CompletionModelDefinition,
    pub model_instance: Box<dyn ModelInstance>,
    pub db_model: Model,
    pub llm_model: ModelMetadata,
}

#[cfg(test)]
mod tests {
    use crate::types::mcp::McpServerType;
    use crate::types::mcp::{McpConfig, McpServerConfig};
    use vllora_llm::mcp::get_tools;

    #[tokio::test]
    async fn test_resolve_mcp_tools_integration() {
        // Connect to a real MCP service (for testing, use mcp.deepwiki.com)
        let mcp_url = "https://mcp.deepwiki.com/mcp".to_string();
        let mcp_server_config = McpServerConfig::new(mcp_url.clone(), McpServerType::Http);

        let mut mcp_config = McpConfig::new();
        mcp_config.add_server("deepwiki".to_string(), mcp_server_config);

        // Attempt to fetch tools from the MCP config definition
        let server_defs = mcp_config.to_mcp_definitions();

        // get_tools returns a Vec of ToolServerTools
        let tools_result = get_tools(&server_defs).await;
        assert!(
            tools_result.is_ok(),
            "Fetching tools from DeepWiki MCP failed: {:?}",
            tools_result.err()
        );
        let all_server_tools = tools_result.unwrap();
        assert!(
            !all_server_tools.is_empty(),
            "No tools received from MCP server"
        );

        // Tools for each server should not be empty
        let mut found_tools = false;
        for server_tools in all_server_tools {
            if !server_tools.tools.is_empty() {
                found_tools = true;
            }
        }
        assert!(
            found_tools,
            "No tools found from https://mcp.deepwiki.com/mcp"
        );
    }
}
