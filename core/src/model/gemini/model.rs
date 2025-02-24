use super::super::error::ModelError;
use super::super::types::{
    LLMContentEvent, LLMFinishEvent, LLMStartEvent, ModelEvent, ModelEventType, ModelFinishReason,
    ModelToolCall,
};
use super::super::ModelInstance;
use super::super::Tool;
use super::client::Client;
use super::types::{
    Content, FinishReason, GenerateContentRequest, GenerateContentResponse, Part,
    PartFunctionResponse, UsageMetadata,
};
use crate::error::GatewayError;
use crate::events::JsonValue;
use crate::events::SPAN_GEMINI;
use crate::events::{self, RecordResult};
use crate::model::error::AuthorizationError;
use crate::model::gemini::types::{FunctionDeclaration, GenerationConfig, Role, Tools};
use crate::model::handler::handle_tool_call;
use crate::model::types::LLMFirstToken;
use crate::model::{async_trait, CredentialsIdent, DEFAULT_MAX_RETRIES};
use crate::types::credentials::ApiKeyCredentials;
use crate::types::engine::{ExecutionOptions, GeminiModelParams, Prompt};
use crate::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, CompletionModelUsage, ToolCall,
};
use crate::types::message::{MessageType, PromptMessage};
use crate::types::threads::{AudioFormat, InnerMessage, Message, MessageContentPartOptions};
use crate::GatewayResult;
use futures::Stream;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::field;
use tracing::Instrument;
use tracing::Span;
use valuable::Valuable;

macro_rules! target {
    () => {
        "langdb::user_tracing::models::gemini"
    };
    ($subtgt:literal) => {
        concat!("langdb::user_tracing::models::gemini::", $subtgt)
    };
}

fn custom_err(e: impl ToString) -> ModelError {
    ModelError::CustomError(e.to_string())
}

pub fn gemini_client(credentials: Option<&ApiKeyCredentials>) -> Result<Client, ModelError> {
    let api_key = if let Some(credentials) = credentials {
        credentials.api_key.clone()
    } else {
        std::env::var("LANGDB_GEMINI_API_KEY").map_err(|_| AuthorizationError::InvalidApiKey)?
    };
    Ok(Client::new(api_key))
}

#[derive(Clone)]
pub struct GeminiModel {
    params: GeminiModelParams,
    execution_options: ExecutionOptions,
    client: Client,
    prompt: Prompt,
    tools: Arc<HashMap<String, Box<dyn Tool>>>,
    credentials_ident: CredentialsIdent,
}
impl GeminiModel {
    pub fn new(
        params: GeminiModelParams,
        execution_options: ExecutionOptions,
        credentials: Option<&ApiKeyCredentials>,
        prompt: Prompt,
        tools: HashMap<String, Box<dyn Tool>>,
    ) -> Result<Self, ModelError> {
        let client = gemini_client(credentials)?;
        Ok(Self {
            params,
            execution_options,
            prompt,
            client,
            tools: Arc::new(tools),
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Langdb),
        })
    }

    async fn handle_tool_calls(
        function_calls: impl Iterator<Item = &(String, HashMap<String, Value>)>,
        tools: &HashMap<String, Box<dyn Tool>>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> Vec<Part> {
        futures::future::join_all(function_calls.map(|(name, args)| {
            let tags = tags.clone();
            async move {
                tracing::trace!("Calling tool  {name:?}");
                let tool_call = Self::map_tool_call(&(name.to_string(), args.clone()));
                let result = handle_tool_call(&tool_call, tools, tx, tags.clone()).await;
                tracing::trace!("Result ({name}): {result:?}");
                let content = result
                    .map(|r| r.to_string())
                    .unwrap_or_else(|err| err.to_string());
                Part::Text(content)
            }
        }))
        .await
    }

    fn build_request(&self, messages: Vec<Content>) -> GatewayResult<GenerateContentRequest> {
        let model_params = &self.params;
        let config = GenerationConfig {
            max_output_tokens: model_params.max_output_tokens,
            temperature: model_params.temperature,
            top_p: model_params.top_p,
            top_k: model_params.top_k,
            stop_sequences: model_params.stop_sequences.clone(),
            candidate_count: model_params.candidate_count,
            presence_penalty: model_params.presence_penalty,
            frequency_penalty: model_params.frequency_penalty,
            seed: model_params.seed,
            response_logprobs: model_params.response_logprobs,
            logprobs: model_params.logprobs,
        };

        let tools = if self.tools.is_empty() {
            None
        } else {
            let mut defs: Vec<FunctionDeclaration> = vec![];

            for (name, tool) in self.tools.iter() {
                let mut params = tool.get_function_parameters().unwrap_or(Default::default());

                if params.r#type == "object" && params.properties.is_empty() {
                    // Gemini throws error if no parameters are defined
                    // GenerateContentRequest.tools[0].function_declarations[0].parameters.properties: should be non-empty for OBJECT type
                    tracing::info!(target: "gemini", "Tool {name} has no parameters defined, using string as fallback");
                    params.r#type = "string".to_string();
                }

                defs.push(FunctionDeclaration {
                    name: name.clone(),
                    description: tool.description(),
                    parameters: params.into(),
                });
            }

            Some(vec![Tools {
                function_declarations: Some(defs),
            }])
        };

        let request = GenerateContentRequest {
            contents: messages,
            generation_config: Some(config),
            tools,
        };

        Ok(request)
    }

    async fn process_stream(
        &self,
        mut stream: impl Stream<Item = Result<Option<GenerateContentResponse>, GatewayError>> + Unpin,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
    ) -> GatewayResult<(
        FinishReason,
        Vec<(String, HashMap<String, Value>)>,
        Option<UsageMetadata>,
    )> {
        let mut calls: Vec<(String, HashMap<String, Value>)> = vec![];
        let mut usage_metadata = None;
        let mut finish_reason = None;
        let mut first_response_received = false;
        while let Some(res) = stream.next().await {
            match res {
                Ok(res) => {
                    if let Some(res) = res {
                        if !first_response_received {
                            first_response_received = true;
                            tx.send(Some(ModelEvent::new(
                                &Span::current(),
                                ModelEventType::LlmFirstToken(LLMFirstToken {}),
                            )))
                            .await
                            .map_err(|e| GatewayError::CustomError(e.to_string()))?;
                        }
                        for candidate in res.candidates {
                            for part in candidate.content.parts {
                                match part {
                                    Part::Text(text) => {
                                        let _ = tx
                                            .send(Some(ModelEvent::new(
                                                &Span::current(),
                                                ModelEventType::LlmContent(LLMContentEvent {
                                                    content: text.to_owned(),
                                                }),
                                            )))
                                            .await;
                                    }
                                    Part::FunctionCall { name, args } => {
                                        calls.push((name.to_string(), args));
                                    }

                                    x => {
                                        return Err(ModelError::StreamError(format!(
                                            "Unexpected stream part: {:?}",
                                            x
                                        ))
                                        .into());
                                    }
                                };
                            }

                            if let Some(reason) = candidate.finish_reason {
                                finish_reason = Some(reason);
                            }
                        }
                        usage_metadata = res.usage_metadata;
                    }
                }
                Err(e) => {
                    tracing::error!("Error in stream: {:?}", e);
                    return Err(e);
                }
            }
        }

        if let Some(reason) = finish_reason {
            return Ok((reason, calls, usage_metadata));
        }
        unreachable!();
    }

    async fn execute(
        &self,
        input_messages: Vec<Content>,
        call_span: Span,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessage> {
        let request = self.build_request(input_messages)?;
        let model_name = self.params.model.as_ref().unwrap();
        let mut gemini_calls = vec![(request, call_span.clone())];
        let mut retries = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some((call, span)) = gemini_calls.pop() {
            if retries == 0 {
                return Err(ModelError::MaxRetriesReached.into());
            } else {
                retries -= 1;
            }
            let model_name = model_name.clone();
            let input_messages = call.contents.clone();

            tx.send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStart(LLMStartEvent {
                    provider_name: SPAN_GEMINI.to_string(),
                    model_name: self.params.model.clone().unwrap_or_default(),
                    input: serde_json::to_string(&input_messages)?,
                }),
            )))
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

            let response = async move {
                let result = self.client.invoke(&model_name, call).await;
                let _ = result
                    .as_ref()
                    .map(|response| serde_json::to_value(response).unwrap())
                    .as_ref()
                    .map(JsonValue)
                    .record();
                let response = result.map_err(custom_err)?;

                let span = Span::current();
                span.record("output", serde_json::to_string(&response)?);
                if let Some(ref usage) = response.usage_metadata {
                    span.record(
                        "usage",
                        JsonValue(&serde_json::to_value(usage).unwrap()).as_value(),
                    );
                }
                Ok::<_, GatewayError>(response)
            }
            .instrument(span.clone().or_current())
            .await?;
            let mut finish_reason = None;
            let mut calls: Vec<(String, HashMap<String, Value>)> = vec![];
            let mut text = String::new();
            for candidate in response.candidates {
                if let Some(reason) = candidate.finish_reason {
                    finish_reason = Some(reason);
                }
                for part in candidate.content.parts {
                    match part {
                        Part::Text(t) => {
                            text.push_str(&t);
                        }
                        Part::FunctionCall { name, args } => {
                            calls.push((name.to_string(), args));
                        }

                        x => {
                            return Err(ModelError::StreamError(format!(
                                "Unexpected stream part: {:?}",
                                x
                            ))
                            .into());
                        }
                    };
                }
            }

            if !calls.is_empty() {
                let mut call_messages = vec![];
                for (name, args) in calls.clone() {
                    call_messages.push(Content {
                        role: Role::Model,
                        parts: vec![Part::FunctionCall { name, args }],
                    });
                }

                let tool_calls_str = serde_json::to_string(
                    &calls
                        .iter()
                        .map(|c| ToolCall {
                            id: c.0.clone(),
                            r#type: "function".to_string(),
                            function: crate::types::gateway::FunctionCall {
                                name: c.0.clone(),
                                arguments: serde_json::to_string(&c.1).unwrap(),
                            },
                        })
                        .collect::<Vec<_>>(),
                )?;
                let tools_span = tracing::info_span!(target: target!(), events::SPAN_TOOLS, tool_calls=tool_calls_str, label=calls.iter().map(|(name, _)| name.clone()).collect::<Vec<String>>().join(","));

                let tool = self.tools.get(&calls[0].0);
                if let Some(tool) = tool {
                    if tool.stop_at_call() {
                        let usage =
                            response
                                .usage_metadata
                                .as_ref()
                                .map(|u| CompletionModelUsage {
                                    input_tokens: u.prompt_token_count as u32,
                                    output_tokens: (u.total_token_count - u.prompt_token_count)
                                        as u32,
                                    total_tokens: u.total_token_count as u32,
                                    ..Default::default()
                                });
                        let finish_reason = ModelFinishReason::ToolCalls;
                        tx.send(Some(ModelEvent::new(
                            &span,
                            ModelEventType::LlmStop(LLMFinishEvent {
                                provider_name: SPAN_GEMINI.to_string(),
                                model_name: self.params.model.clone().unwrap_or_default(),
                                output: Some(text.clone()),
                                usage,
                                finish_reason,
                                tool_calls: calls
                                    .iter()
                                    .map(|(tool_name, params)| {
                                        Ok(ModelToolCall {
                                            tool_id: tool_name.clone(),
                                            tool_name: tool_name.clone(),
                                            input: serde_json::to_string(params)?,
                                        })
                                    })
                                    .collect::<Result<Vec<ModelToolCall>, GatewayError>>()?,
                                credentials_ident: self.credentials_ident.clone(),
                            }),
                        )))
                        .await
                        .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                        return Ok(ChatCompletionMessage {
                            role: "assistant".to_string(),
                            content: if text.is_empty() {
                                None
                            } else {
                                Some(ChatCompletionContent::Text(text.clone()))
                            },
                            tool_calls: Some(
                                calls
                                    .iter()
                                    .map(|(tool_name, params)| {
                                        Ok(ToolCall {
                                            id: tool_name.clone(),
                                            r#type: "function".to_string(),
                                            function: crate::types::gateway::FunctionCall {
                                                name: tool_name.clone(),
                                                arguments: serde_json::to_string(params)?,
                                            },
                                        })
                                    })
                                    .collect::<Result<Vec<ToolCall>, GatewayError>>()?,
                            ),
                            ..Default::default()
                        });
                    }
                }
                tools_span.follows_from(span.id());
                let tool_call_parts =
                    Self::handle_tool_calls(calls.iter(), &self.tools, tx, tags.clone())
                        .instrument(tools_span.clone())
                        .await;
                let tools_messages = vec![Content {
                    role: Role::User,
                    parts: tool_call_parts,
                }];

                let conversation_messages =
                    [input_messages, call_messages, tools_messages].concat();
                let request = self.build_request(conversation_messages)?;
                let input = serde_json::to_string(&request)?;
                let call_span = tracing::info_span!(
                    target: target!("chat"),
                    SPAN_GEMINI,
                    ttft = field::Empty,
                    input=input,
                    output = field::Empty,
                    error = field::Empty,
                    usage = field::Empty,
                    request = JsonValue(&serde_json::to_value(&request).unwrap_or_default()).as_value()
                );
                call_span.follows_from(tools_span.id());
                gemini_calls.push((request, call_span));
                continue;
            }

            match finish_reason {
                Some(FinishReason::Stop) => {
                    let usage = response
                        .usage_metadata
                        .as_ref()
                        .map(|u| CompletionModelUsage {
                            input_tokens: u.prompt_token_count as u32,
                            output_tokens: (u.total_token_count - u.prompt_token_count) as u32,
                            total_tokens: u.total_token_count as u32,
                            ..Default::default()
                        });

                    tx.send(Some(ModelEvent::new(
                        &span,
                        ModelEventType::LlmStop(LLMFinishEvent {
                            provider_name: SPAN_GEMINI.to_string(),
                            model_name: self
                                .params
                                .model
                                .clone()
                                .map(|m| m.to_string())
                                .unwrap_or_default(),
                            output: Some(text.clone()),
                            usage,
                            finish_reason: ModelFinishReason::Stop,
                            tool_calls: vec![],
                            credentials_ident: self.credentials_ident.clone(),
                        }),
                    )))
                    .await
                    .map_err(|e| GatewayError::CustomError(e.to_string()))?;

                    return Ok(ChatCompletionMessage {
                        role: "assistant".to_string(),
                        content: Some(ChatCompletionContent::Text(text)),
                        ..Default::default()
                    });
                }
                _ => {
                    let err = Self::handle_finish_reason(finish_reason);

                    return Err(err);
                }
            };
        }
        unreachable!();
    }

    fn handle_finish_reason(finish_reason: Option<FinishReason>) -> GatewayError {
        match finish_reason {
            Some(FinishReason::MaxTokens) => GatewayError::ModelError(ModelError::FinishError(
                "the maximum number of tokens specified in the request was reached".to_string(),
            )),
            x => GatewayError::ModelError(ModelError::FinishError(format!("{x:?}"))),
        }
    }

    fn map_finish_reason(finish_reason: &FinishReason, has_tool_calls: bool) -> ModelFinishReason {
        match finish_reason {
            FinishReason::FinishReasonUnspecified => {
                ModelFinishReason::Other("Unspecified".to_string())
            }
            FinishReason::Stop => {
                if has_tool_calls {
                    ModelFinishReason::ToolCalls
                } else {
                    ModelFinishReason::Stop
                }
            }
            FinishReason::MaxTokens => ModelFinishReason::Length,
            FinishReason::Safety => ModelFinishReason::ContentFilter,
            FinishReason::Recitation => ModelFinishReason::Other("Recitation".to_string()),
            FinishReason::Other => ModelFinishReason::Other("Other".to_string()),
        }
    }

    fn map_usage(usage: Option<&UsageMetadata>) -> Option<CompletionModelUsage> {
        usage.map(|u| CompletionModelUsage {
            input_tokens: u.prompt_token_count as u32,
            output_tokens: (u.total_token_count - u.prompt_token_count) as u32,
            total_tokens: u.total_token_count as u32,
            ..Default::default()
        })
    }

    fn map_tool_call(t: &(String, HashMap<String, Value>)) -> ModelToolCall {
        ModelToolCall {
            tool_id: t.0.clone(),
            tool_name: t.0.clone(),
            input: serde_json::to_string(&t.1).unwrap(),
        }
    }

    async fn execute_stream(
        &self,
        input_messages: Vec<Content>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        call_span: Span,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        let model_name = self.params.model.as_ref().unwrap();
        let request = self.build_request(input_messages)?;
        let mut gemini_calls = vec![(request, call_span)];

        let mut retries = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some((call, span)) = gemini_calls.pop() {
            if retries == 0 {
                return Err(ModelError::MaxRetriesReached.into());
            } else {
                retries -= 1;
            }
            tracing::trace!("Call: {:?}", call);
            let model_name = model_name.clone();
            let input_messages = call.contents.clone();
            let stream = self.client.stream(&model_name, call).await?;
            tokio::pin!(stream);
            tx.send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStart(LLMStartEvent {
                    provider_name: SPAN_GEMINI.to_string(),
                    model_name: self
                        .params
                        .model
                        .clone()
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                    input: serde_json::to_string(&input_messages)?,
                }),
            )))
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

            let (finish_reason, tool_calls, usage) = self
                .process_stream(stream, &tx)
                .instrument(span.clone())
                .await?;

            let trace_finish_reason =
                Self::map_finish_reason(&finish_reason, !tool_calls.is_empty());
            let usage = Self::map_usage(usage.as_ref());
            if let Some(usage) = &usage {
                span.record("usage", JsonValue(&serde_json::to_value(usage)?).as_value());
            }
            tx.send(Some(ModelEvent::new(
                &span,
                ModelEventType::LlmStop(LLMFinishEvent {
                    provider_name: SPAN_GEMINI.to_string(),
                    model_name: self
                        .params
                        .model
                        .clone()
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                    output: None,
                    usage,
                    finish_reason: trace_finish_reason.clone(),
                    tool_calls: tool_calls.iter().map(Self::map_tool_call).collect(),
                    credentials_ident: self.credentials_ident.clone(),
                }),
            )))
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))?;

            let response = serde_json::json!({
                "finish_reason": finish_reason,
                "tool_calls": tool_calls
            });
            span.record("output", response.to_string());
            if !tool_calls.is_empty() {
                let mut call_messages = vec![];
                let mut tools = vec![];
                for (name, args) in tool_calls.clone() {
                    tools.push(ToolCall {
                        id: name.clone(),
                        r#type: "function".to_string(),
                        function: crate::types::gateway::FunctionCall {
                            name: name.clone(),
                            arguments: serde_json::to_string(&args)?,
                        },
                    });
                    call_messages.push(Content {
                        role: Role::Model,
                        parts: vec![Part::FunctionCall { name, args }],
                    });
                }
                let tool_calls_str = serde_json::to_string(&tools)?;

                let tools_span = tracing::info_span!(target: target!(), events::SPAN_TOOLS, tool_calls=tool_calls_str, label=tool_calls.iter().map(|(name, _)| name.clone()).collect::<Vec<String>>().join(","));
                let tool = self.tools.get(&tool_calls[0].0);
                if let Some(tool) = tool {
                    if tool.stop_at_call() {
                        return Ok(());
                    }
                }

                tools_span.follows_from(span.id());
                let tool_call_parts =
                    Self::handle_tool_calls(tool_calls.iter(), &self.tools, &tx, tags.clone())
                        .instrument(tools_span.clone())
                        .await;
                let tools_messages = vec![Content {
                    role: Role::User,
                    parts: tool_call_parts,
                }];

                let conversation_messages =
                    [input_messages, call_messages, tools_messages].concat();
                tracing::trace!("New messages: {conversation_messages:?}");
                let request = self.build_request(conversation_messages)?;
                let input = serde_json::to_string(&request)?;
                let call_span = tracing::info_span!(target: target!("chat"), SPAN_GEMINI, ttft = field::Empty, input = input,output = field::Empty, error = field::Empty, usage = field::Empty);
                call_span.follows_from(tools_span.id());
                gemini_calls.push((request, call_span));
                continue;
            }
            match finish_reason {
                FinishReason::Stop => return Ok(()),
                other => {
                    return Err(Self::handle_finish_reason(Some(other)));
                }
            }
        }

        Ok(())
    }

    fn map_previous_messages(messages_dto: Vec<Message>) -> GatewayResult<Vec<Content>> {
        // convert serde::Map into HashMap
        let mut messages = vec![];
        let mut tool_results_remaining = 0;
        let mut tool_calls_collected = vec![];
        for m in messages_dto.iter() {
            let request_message = {
                match m.r#type {
                    MessageType::SystemMessage => {
                        Some(Content::user(m.content.clone().unwrap_or_default()))
                    }

                    MessageType::AIMessage => {
                        if let Some(tool_calls) = &m.tool_calls {
                            tool_results_remaining = tool_calls.len();
                            tool_calls_collected = vec![];
                            Some(Content {
                                role: Role::Model,
                                parts: tool_calls
                                    .iter()
                                    .map(|c| {
                                        Ok(Part::FunctionCall {
                                            name: c.id.clone(),
                                            args: serde_json::from_str(&c.function.arguments)?,
                                        })
                                    })
                                    .collect::<Result<Vec<Part>, GatewayError>>()?,
                            })
                        } else {
                            match &m.content {
                                Some(content) if !content.is_empty() => {
                                    Some(Content::model(content.clone()))
                                }
                                _ => None,
                            }
                        }
                    }
                    MessageType::HumanMessage => Some(construct_user_message(&m.clone().into())),
                    MessageType::ToolResult => {
                        tool_results_remaining -= 1;
                        let content =
                            serde_json::to_value(m.content.clone().unwrap_or_default()).unwrap();
                        tool_calls_collected.push(Part::FunctionResponse {
                            name: m.tool_call_id.clone().unwrap_or_default(),
                            response: Some(PartFunctionResponse {
                                fields: HashMap::from([(
                                    "content
                                "
                                    .to_string(),
                                    content,
                                )]),
                            }),
                        });
                        if tool_results_remaining == 0 {
                            Some(Content {
                                role: Role::User,
                                parts: tool_calls_collected.clone(),
                            })
                        } else {
                            None
                        }
                    }
                }
            };

            if let Some(request_message) = request_message {
                messages.push(request_message);
            }
        }

        Ok(messages)
    }
}

#[async_trait]
impl ModelInstance for GeminiModel {
    async fn invoke(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<ChatCompletionMessage> {
        let conversational_messages =
            self.construct_messages(input_variables, previous_messages)?;
        let input = serde_json::to_string(&conversational_messages)?;
        let call_span = tracing::info_span!(target: target!("chat"), SPAN_GEMINI, ttft = field::Empty, input = input, output = field::Empty, error = field::Empty, usage = field::Empty, tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value());
        self.execute(conversational_messages, call_span.clone(), &tx, tags)
            .instrument(call_span.clone())
            .await
            .map_err(|e| record_map_err(e, call_span))
    }

    async fn stream(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> GatewayResult<()> {
        let conversational_messages =
            self.construct_messages(input_variables, previous_messages)?;
        let input = serde_json::to_string(&conversational_messages)?;
        let call_span = tracing::info_span!(target: target!("chat"), SPAN_GEMINI, ttft = field::Empty, input = input, output = field::Empty, error = field::Empty, usage = field::Empty, tags = JsonValue(&serde_json::to_value(tags.clone()).unwrap_or_default()).as_value());
        self.execute_stream(conversational_messages, tx, call_span.clone(), tags)
            .instrument(call_span.clone())
            .await
            .map_err(|e| record_map_err(e, call_span))
    }
}

impl GeminiModel {
    fn construct_messages(
        &self,
        input_variables: HashMap<String, Value>,
        previous_messages: Vec<Message>,
    ) -> GatewayResult<Vec<Content>> {
        let mut conversational_messages = vec![];
        let system_message = self
            .prompt
            .messages
            .iter()
            .find(|m| m.r#type == MessageType::SystemMessage)
            .map(|message| map_chat_messages(message.to_owned(), &input_variables));
        if let Some(system_message) = system_message {
            conversational_messages.push(system_message?);
        }
        let previous_messages = Self::map_previous_messages(previous_messages)?;
        conversational_messages.extend(previous_messages);
        let human_message = self
            .prompt
            .messages
            .iter()
            .find(|m| m.r#type == MessageType::HumanMessage)
            .map(|message| map_chat_messages(message.to_owned(), &input_variables));
        if let Some(human_message) = human_message {
            conversational_messages.push(human_message?);
        }

        Ok(conversational_messages)
    }
}

fn map_chat_messages(
    prompt: PromptMessage,
    variables: &HashMap<String, Value>,
) -> GatewayResult<Content> {
    let message = match prompt.r#type {
        MessageType::AIMessage => {
            let raw_message = Prompt::render(prompt.msg, variables.clone());
            Content::model(raw_message)
        }
        MessageType::SystemMessage => {
            let raw_message = Prompt::render(prompt.msg, variables.clone());
            Content::user(raw_message)
        }
        MessageType::HumanMessage => {
            let msg = prompt.msg;
            let inner_message: InnerMessage = if prompt.wired {
                let value = variables
                    .get(&msg)
                    .ok_or(GatewayError::CustomError(format!("{msg} not specified")))?;
                serde_json::from_value(value.clone())?
            } else {
                InnerMessage::Text(Prompt::render(msg, variables.clone()))
            };
            construct_user_message(&inner_message)
        }
        MessageType::ToolResult => {
            todo!()
        }
    };
    Ok(message)
}

fn construct_user_message(m: &InnerMessage) -> Content {
    match m {
        crate::types::threads::InnerMessage::Text(text) => Content::user(text.to_string()),
        crate::types::threads::InnerMessage::Array(content_array) => {
            let mut parts = vec![];
            for m in content_array {
                let msg: Part = match m.r#type {
                    crate::types::threads::MessageContentType::Text => Part::Text(m.value.clone()),
                    crate::types::threads::MessageContentType::ImageUrl => {
                        let url = m.value.clone();
                        let base64_data = url
                            .split_once(',')
                            .map_or_else(|| url.as_str(), |(_, data)| data);
                        Part::InlineData {
                            mime_type: "image/png".to_string(),
                            data: base64_data.to_string(),
                        }
                    }
                    crate::types::threads::MessageContentType::InputAudio => {
                        let mut format = "mp3".to_string();

                        if let Some(MessageContentPartOptions::Audio(a)) = &m.additional_options {
                            format = match a.r#type {
                                AudioFormat::Mp3 => "mp3".to_string(),
                                AudioFormat::Wav => "wav".to_string(),
                            }
                        }

                        Part::InlineData {
                            mime_type: format!("audio/{format}"),
                            data: m.value.to_string(),
                        }
                    }
                };
                parts.push(msg)
            }
            Content {
                role: Role::User,
                parts,
            }
        }
    }
}

pub fn record_map_err(e: impl Into<GatewayError> + ToString, span: tracing::Span) -> GatewayError {
    span.record("error", e.to_string());
    e.into()
}
