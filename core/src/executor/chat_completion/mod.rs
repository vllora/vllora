use crate::executor::chat_completion::stream_executor::stream_chunks;
use crate::handler::{find_model_by_full_name, ModelEventWithDetails};
use crate::llm_gateway::message_mapper::MessageMapper;
use crate::llm_gateway::provider::Provider;
use crate::model::mcp::get_tools;
use crate::model::tools::{GatewayTool, Tool};
use crate::model::types::ModelEvent;
use crate::model::types::ModelEventType;
use crate::model::ModelInstance;
use crate::models::ModelMetadata;
use crate::types::engine::{
    CompletionModelDefinition, CompletionModelParams, ExecutionOptions, Model, ModelTool,
    ModelTools, ModelType, Prompt,
};
use crate::types::gateway::{
    ChatCompletionChoice, ChatCompletionContent, ChatCompletionDelta, ChatCompletionMessage,
    ChatCompletionRequestWithTools, ChatCompletionResponse, ChatCompletionUsage, Extra,
    GuardOrName, GuardWithParameters,
};
use crate::types::guardrails::service::GuardrailsEvaluator;
use crate::types::guardrails::{GuardError, GuardResult, GuardStage};
use crate::GatewayApiError;

use either::Either::{self, Left, Right};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use stream_wrapper::wrap_stream;
use tokio_stream::wrappers::ReceiverStream;
use tracing::Span;
use tracing_futures::Instrument;
use uuid::Uuid;

use super::context::ExecutorContext;
use super::{get_key_credentials, use_langdb_proxy};
use crate::executor::chat_completion::stream_wrapper::ChatCompletionStream;

pub mod basic_executor;
pub mod routed_executor;
pub mod stream_executor;
pub mod stream_wrapper;

pub async fn execute<T: Serialize + DeserializeOwned + Debug + Clone>(
    request_with_tools: &ChatCompletionRequestWithTools<T>,
    executor_context: &ExecutorContext,
    router_span: tracing::Span,
) -> Result<
    Either<
        Result<ChatCompletionStream, GatewayApiError>,
        Result<ChatCompletionResponse, GatewayApiError>,
    >,
    GatewayApiError,
> {
    let span = Span::current();

    let mut request_tools = vec![];
    let mut tools_map = HashMap::new();
    if let Some(tools) = &request_with_tools.request.tools {
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

    let mcp_tools = match &request_with_tools.mcp_servers {
        Some(tools) => get_tools(tools).await?,
        None => Vec::new(),
    };

    for server_tools in mcp_tools {
        for tool in server_tools.tools {
            tools_map.insert(tool.name(), Box::new(tool.clone()) as Box<dyn Tool>);
            request_tools.push(tool.into());
        }
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<ModelEvent>>(1000);

    let tools = ModelTools(request_tools);

    let resolved_model_context = resolve_model_instance(
        executor_context,
        request_with_tools,
        tools_map,
        tools,
        router_span,
    )
    .await?;

    let mut request = request_with_tools.request.clone();
    let llm_model = find_model_by_full_name(&request.model, &executor_context.provided_models)?;
    request.model = llm_model.inference_provider.model_name.clone();

    let user: String = request
        .user
        .as_ref()
        .map_or(Uuid::new_v4().to_string(), |v| v.clone());

    let mut messages = vec![];

    for message in &request.messages {
        messages.push(MessageMapper::map_completions_message_to_langdb_message(
            message,
            &request.model,
            &user.to_string(),
        )?);
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

            ch.on_message(ModelEventWithDetails::new(msg, db_model.clone()));
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

    let result = apply_guardrails(
        &request_with_tools.request.messages,
        request_with_tools.extra.as_ref(),
        executor_context.evaluator_service.as_ref().as_ref(),
        executor_context,
        GuardStage::Input,
    )
    .instrument(span.clone())
    .await;

    let result = resolve_guard_result(result, &GuardStage::Input, None)?;
    if let Some(r) = result {
        if is_stream {
            return Ok(Left(stream_response_to_stream(r)));
        } else {
            return Ok(Right(Ok(r)));
        }
    }

    if is_stream {
        Ok(Left(
            stream_chunks(
                resolved_model_context.completion_model_definition,
                resolved_model_context.model_instance,
                messages.clone(),
                executor_context.callbackhandler.clone().into(),
                executor_context.tags.clone(),
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
        )
        .instrument(span)
        .await;

        if let Ok(completion_response) = &result {
            let ChatCompletionResponse { choices, .. } = completion_response;
            for choice in choices {
                let result = apply_guardrails(
                    &[choice.message.clone()],
                    request_with_tools.extra.as_ref(),
                    executor_context.evaluator_service.as_ref().as_ref(),
                    executor_context,
                    GuardStage::Output,
                )
                .await;

                let result = resolve_guard_result(
                    result,
                    &GuardStage::Output,
                    Some(completion_response.clone()),
                )?;
                if let Some(r) = result {
                    return Ok(Right(Ok(r)));
                }
            }
        }

        Ok(Right(result))
    }
}

pub async fn apply_guardrails(
    messages: &[ChatCompletionMessage],
    extra: Option<&Extra>,
    evaluator: &dyn GuardrailsEvaluator,
    executor_context: &ExecutorContext,
    guard_stage: GuardStage,
) -> Result<(), GuardError> {
    let Some(Extra { guards, .. }) = extra else {
        return Ok(());
    };

    for guard in guards {
        let (guard_id, parameters) = match guard {
            GuardOrName::GuardId(guard_id) => (guard_id, None),
            GuardOrName::GuardWithParameters(GuardWithParameters { id, parameters }) => {
                (id, Some(parameters))
            }
        };

        let result = evaluator
            .evaluate(
                messages,
                guard_id,
                executor_context,
                parameters,
                &guard_stage,
            )
            .await
            .map_err(GuardError::GuardEvaluationError)?;

        match result {
            GuardResult::Json { passed, .. }
            | GuardResult::Boolean { passed, .. }
            | GuardResult::Text { passed, .. }
                if !passed =>
            {
                return Err(GuardError::GuardNotPassed(guard_id.clone(), result));
            }
            _ => {}
        }
    }

    Ok(())
}

pub async fn resolve_model_instance<T: Serialize + DeserializeOwned + Debug + Clone>(
    executor_context: &ExecutorContext,
    request: &ChatCompletionRequestWithTools<T>,
    tools_map: HashMap<String, Box<dyn Tool>>,
    tools: ModelTools,
    router_span: Span,
) -> Result<ResolvedModelContext, GatewayApiError> {
    let llm_model =
        find_model_by_full_name(&request.request.model, &executor_context.provided_models)?;
    let (key_credentials, llm_model) = use_langdb_proxy(executor_context, llm_model.clone());

    let key = get_key_credentials(
        key_credentials.as_ref(),
        executor_context.providers_config.as_ref(),
        &llm_model.inference_provider.provider.to_string(),
    );
    let provider_specific = request.provider_specific.clone();
    let request = request.request.clone();

    let engine = Provider::get_completion_engine_for_model(
        &llm_model,
        &request,
        key.clone(),
        provider_specific.as_ref(),
    )?;

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

    let model_instance = crate::model::init_completion_model_instance(
        completion_model_definition.clone(),
        tools_map,
        Some(executor_context.cost_calculator.clone()),
        llm_model.inference_provider.endpoint.as_deref(),
        Some(&llm_model.inference_provider.provider.to_string()),
        router_span.clone(),
    )
    .await
    .map_err(|e| GatewayApiError::CustomError(e.to_string()))?;

    Ok(ResolvedModelContext {
        completion_model_definition,
        model_instance,
        db_model,
        llm_model,
    })
}

pub struct ResolvedModelContext {
    pub completion_model_definition: CompletionModelDefinition,
    pub model_instance: Box<dyn ModelInstance>,
    pub db_model: Model,
    pub llm_model: ModelMetadata,
}

fn stream_response_to_stream(
    response: ChatCompletionResponse,
) -> Result<ChatCompletionStream, GatewayApiError> {
    let (tx, rx) = tokio::sync::mpsc::channel(1);

    tokio::spawn(async move {
        if let Some(choice) = response.choices.first() {
            // Send refusal if present
            if let Some(refusal) = &choice.message.refusal {
                let _ = tx
                    .send(Ok((
                        Some(ChatCompletionDelta {
                            role: Some("assistant".to_string()),
                            content: Some(refusal.clone()),
                            tool_calls: None,
                        }),
                        None,
                        choice.finish_reason.clone(),
                    )))
                    .await;
            } else {
                // Send content if present
                if let Some(content) = &choice.message.content {
                    let content = match content {
                        ChatCompletionContent::Text(text) => text.clone(),
                        _ => unreachable!(), // Not supported in streaming
                    };

                    let _ = tx
                        .send(Ok((
                            Some(ChatCompletionDelta {
                                role: Some("assistant".to_string()),
                                content: Some(content),
                                tool_calls: None,
                            }),
                            None,
                            choice.finish_reason.clone(),
                        )))
                        .await;
                }
            }
        }
    });

    Ok(wrap_stream(ReceiverStream::new(rx)))
}

fn resolve_guard_result(
    result: Result<(), GuardError>,
    guard_stage: &GuardStage,
    response: Option<ChatCompletionResponse>,
) -> Result<Option<ChatCompletionResponse>, GatewayApiError> {
    let response = response.unwrap_or(ChatCompletionResponse {
        id: "".to_string(),
        object: "chat.completion".to_string(),
        created: 0,
        model: "".to_string(),
        usage: ChatCompletionUsage::default(),
        choices: vec![],
    });

    match result {
        Ok(_) => Ok(None),
        Err(GuardError::GuardNotPassed(_, _)) => {
            let stage = match guard_stage {
                GuardStage::Input => "Input",
                GuardStage::Output => "Output",
            };
            let finish_reason = match guard_stage {
                GuardStage::Input => "rejected",
                GuardStage::Output => "stop",
            };
            Ok(Some(ChatCompletionResponse {
                choices: vec![ChatCompletionChoice {
                    message: ChatCompletionMessage {
                        role: "assistant".to_string(),
                        content: Some(ChatCompletionContent::Text(format!(
                            "{} rejected by guard",
                            stage
                        ))),
                        ..Default::default()
                    },
                    finish_reason: Some(finish_reason.to_string()),
                    index: 0,
                }],
                id: response.id.clone(),
                object: response.object.clone(),
                created: response.created,
                model: response.model.clone(),
                usage: response.usage.clone(),
            }))
        }
        Err(e) => Err(GatewayApiError::GuardError(e)),
    }
}
