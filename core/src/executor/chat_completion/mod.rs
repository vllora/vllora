pub mod basic_executor;
pub mod routed_executor;
pub mod stream_executor;

use std::collections::HashMap;
use std::sync::Arc;

use crate::executor::ProvidersConfig;
use crate::llm_gateway::message_mapper::MessageMapper;
use crate::llm_gateway::provider::Provider;
use crate::model::mcp::get_mcp_tools;
use crate::model::tools::{GatewayTool, Tool};
use crate::model::types::ModelEvent;
use crate::models::ModelDefinition;
use crate::types::gateway::{ChatCompletionRequestWithTools, CompletionModelUsage};
use actix_web::{HttpMessage, HttpRequest};
use either::Either::{self, Left, Right};
use futures::Stream;
use uuid::Uuid;

use crate::{
    model::types::ModelEventType,
    types::{
        credentials::Credentials,
        engine::{
            CompletionModelDefinition, CompletionModelParams, ExecutionOptions, Model, ModelTool,
            ModelTools, ModelType, Prompt,
        },
        gateway::{ChatCompletionDelta, ChatCompletionResponse, CostCalculator},
    },
};
use tracing::Span;
use tracing_futures::Instrument;

use crate::executor::chat_completion::stream_executor::stream_chunks;
use crate::handler::extract_tags;
use crate::handler::{CallbackHandlerFn, ModelEventWithDetails};
use crate::GatewayApiError;

use super::get_key_credentials;

pub async fn execute(
    request: &ChatCompletionRequestWithTools,
    callback_handler: &CallbackHandlerFn,
    req: HttpRequest,
    cost_calculator: Arc<Box<dyn CostCalculator>>,
    llm_model: &ModelDefinition,
) -> Result<
    Either<
        Result<
            impl Stream<
                Item = Result<
                    (Option<ChatCompletionDelta>, Option<CompletionModelUsage>),
                    GatewayApiError,
                >,
            >,
            GatewayApiError,
        >,
        Result<ChatCompletionResponse, GatewayApiError>,
    >,
    GatewayApiError,
> {
    let span = Span::current();
    let tags = extract_tags(&req)?;

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
                Box::new(GatewayTool { def: tool.clone() }) as Box<dyn Tool>,
            );
        }
    }

    let mcp_tools = match &request.mcp_servers {
        Some(tools) => get_mcp_tools(tools).await?,
        None => Vec::new(),
    };

    for server_tools in mcp_tools {
        for tool in server_tools.tools {
            tools_map.insert(tool.name(), Box::new(tool.clone()) as Box<dyn Tool>);
            request_tools.push(tool.into());
        }
    }

    let mut request = request.request.clone();

    request.model = llm_model.inference_provider.model_name.clone();

    let user: String = request
        .user
        .as_ref()
        .map_or(Uuid::new_v4().to_string(), |v| v.clone());

    let key_credentials = req.extensions().get::<Credentials>().cloned();
    let providers_config = req.app_data::<ProvidersConfig>().cloned();
    let key = get_key_credentials(
        key_credentials.as_ref(),
        providers_config.as_ref(),
        &llm_model.inference_provider.provider.to_string(),
    );
    let engine = Provider::get_completion_engine_for_model(llm_model, &request, key.clone())?;

    let tools = ModelTools(request_tools);

    let db_model = Model {
        name: request.model.clone(),
        description: Some("Generated model for chat completion".to_string()),
        provider_name: llm_model.inference_provider.provider.to_string(),
        prompt_name: None,
        model_params: HashMap::new(),
        execution_options: ExecutionOptions::default(),
        tools: tools.clone(),
        model_type: ModelType::Completions,
        response_schema: None,
        credentials: key,
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
        prompt: Prompt::empty(),
        tools,
        db_model: db_model.clone(),
    };

    let model = crate::model::init_completion_model_instance(
        completion_model_definition.clone(),
        tools_map,
        Some(cost_calculator.clone()),
        llm_model.inference_provider.endpoint.as_deref(),
        Some(&llm_model.inference_provider.provider.to_string()),
    )
    .await
    .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    let mut messages = vec![];

    for message in &request.messages {
        messages.push(MessageMapper::map_completions_message_to_langdb_message(
            message,
            &request.model,
            &user.to_string(),
        )?);
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(1000);

    let ch = callback_handler.clone();
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

            if let ModelEvent {
                event: ModelEventType::LlmFirstToken(e),
                ..
            } = &msg
            {
                let current_span = Span::current();
                current_span.record("ttft", e.ttft);
            }

            ch.on_message(ModelEventWithDetails::new(msg, db_model.clone()));
        }

        (stop_event, tool_calls)
    });

    if request.stream.unwrap_or(false) {
        Ok(Left(
            stream_chunks(
                completion_model_definition,
                model,
                messages.clone(),
                callback_handler.clone().into(),
                tags.clone(),
            )
            .instrument(span)
            .await,
        ))
    } else {
        Ok(Right(
            basic_executor::execute(
                request,
                model,
                messages.clone(),
                tags.clone(),
                tx,
                span.clone(),
                handle,
            )
            .instrument(span)
            .await,
        ))
    }
}
