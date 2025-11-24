use super::client::Client;
use super::types::{
    Content, FinishReason, GenerateContentRequest, GenerateContentResponse, Part,
    PartFunctionResponse, UsageMetadata,
};
use crate::client::error::AuthorizationError;
use crate::client::error::ModelError;
use crate::client::tools::handler::handle_tool_call;
use crate::client::DEFAULT_MAX_RETRIES;
use crate::error::LLMError;
use crate::error::LLMResult;
use crate::error::ModelFinishError;
use crate::provider::gemini::types::{
    Candidate, FunctionDeclaration, GenerationConfig, PartWithThought, Role, Tools,
};
use crate::types::credentials::ApiKeyCredentials;
use crate::types::credentials_ident::CredentialsIdent;
use crate::types::engine::render;
use crate::types::engine::{ExecutionOptions, GeminiModelParams};
use crate::types::gateway::{
    ChatCompletionContent, ChatCompletionMessage, ChatCompletionMessageWithFinishReason,
    CompletionModelUsage, ToolCall,
};
use crate::types::instance::ModelInstance;
use crate::types::message::{AudioFormat, InnerMessage, Message, MessageContentPartOptions};
use crate::types::message::{MessageContentType, MessageType};
use crate::types::tools::Tool;
use crate::types::{GoogleToolCallExtra, LLMFirstToken, ToolCallExtra};
use crate::types::{
    LLMContentEvent, LLMFinishEvent, LLMStartEvent, ModelEvent, ModelEventType, ModelFinishReason,
    ModelToolCall,
};
use async_openai::types::ResponseFormat;
use async_trait::async_trait;
use futures::Stream;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::field;
use tracing::Instrument;
use tracing::Span;
use valuable::Valuable;
use vllora_telemetry::create_model_span;
use vllora_telemetry::events::JsonValue;
use vllora_telemetry::events::SPAN_GEMINI;
use vllora_telemetry::events::{self, RecordResult};

macro_rules! target {
    () => {
        "vllora::user_tracing::models::gemini"
    };
    ($subtgt:literal) => {
        concat!("vllora::user_tracing::models::gemini::", $subtgt)
    };
}

fn custom_err(e: impl ToString) -> ModelError {
    ModelError::CustomError(e.to_string())
}

fn map_calls_to_tool_names(calls: &[(String, HashMap<String, Value>, Option<String>)]) -> String {
    calls
        .iter()
        .map(|(name, _, _)| name.clone())
        .collect::<std::collections::HashSet<String>>()
        .into_iter()
        .collect::<Vec<String>>()
        .join(",")
}

pub fn gemini_client(credentials: Option<&ApiKeyCredentials>) -> Result<Client, ModelError> {
    let api_key = if let Some(credentials) = credentials {
        credentials.api_key.clone()
    } else {
        std::env::var("VLLORA_GEMINI_API_KEY").map_err(|_| AuthorizationError::InvalidApiKey)?
    };
    Ok(Client::new(api_key))
}

enum InnerExecutionResult {
    Finish(Box<ChatCompletionMessageWithFinishReason>),
    NextCall(Vec<Content>),
}

#[derive(Clone)]
pub struct GeminiModel {
    params: GeminiModelParams,
    execution_options: ExecutionOptions,
    client: Client,
    tools: Arc<HashMap<String, Box<dyn Tool>>>,
    credentials_ident: CredentialsIdent,
}
impl GeminiModel {
    pub fn new(
        params: GeminiModelParams,
        execution_options: ExecutionOptions,
        credentials: Option<&ApiKeyCredentials>,
        tools: HashMap<String, Box<dyn Tool>>,
    ) -> Result<Self, ModelError> {
        let client = gemini_client(credentials)?;
        Ok(Self {
            params,
            execution_options,
            client,
            tools: Arc::new(tools),
            credentials_ident: credentials
                .map(|_c| CredentialsIdent::Own)
                .unwrap_or(CredentialsIdent::Vllora),
        })
    }

    async fn handle_tool_calls(
        function_calls: impl Iterator<Item = &(String, HashMap<String, Value>, Option<String>)>,
        tools: &HashMap<String, Box<dyn Tool>>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> Vec<PartWithThought> {
        futures::future::join_all(function_calls.map(|(name, args, thought_signature)| {
            let tags = tags.clone();
            async move {
                tracing::trace!("Calling tool  {name:?}");
                let tool_call = Self::map_tool_call(&(
                    name.to_string(),
                    args.clone(),
                    thought_signature.clone(),
                ));
                let result = handle_tool_call(&tool_call, tools, tx, tags.clone()).await;
                tracing::trace!("Result ({name}): {result:?}");
                let content = result
                    .map(|r| r.to_string())
                    .unwrap_or_else(|err| err.to_string());
                Part::Text(content).into()
            }
        }))
        .await
    }

    fn build_request(&self, messages: Vec<Content>) -> LLMResult<GenerateContentRequest> {
        let model_params = &self.params;
        let response_schema = match &model_params.response_format {
            Some(ResponseFormat::JsonSchema { json_schema }) => {
                let schema = json_schema.schema.clone();

                if let Some(s) = &schema {
                    let s = replace_refs_with_defs(s.clone());
                    let s = remove_additional_properties(s.clone());
                    let s = normalize_nullable_types(s.clone());
                    Some(s)
                } else {
                    schema
                }
            }
            _ => None,
        };
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
            response_mime_type: if response_schema.is_some() {
                Some("application/json".to_string())
            } else {
                None
            },
            response_schema,
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
        mut stream: impl Stream<Item = Result<Option<GenerateContentResponse>, LLMError>> + Unpin,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        started_at: std::time::Instant,
    ) -> LLMResult<(
        FinishReason,
        Vec<(String, HashMap<String, Value>, Option<String>)>,
        Option<UsageMetadata>,
        Option<GenerateContentResponse>,
    )> {
        let mut calls: Vec<(String, HashMap<String, Value>, Option<String>)> = vec![];
        let mut usage_metadata = None;
        let mut finish_reason = None;
        let mut first_response_received = false;
        let mut content = String::new();
        let mut model_version = String::new();
        let mut response_id = String::new();
        let mut last_chunk = None;
        while let Some(res) = stream.next().await {
            last_chunk = Some(res.as_ref().map_or_else(
                |e| e.to_string(),
                |r| serde_json::to_string(r).unwrap_or_default(),
            ));
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
                            .map_err(|e| LLMError::CustomError(e.to_string()))?;
                            model_version = res.model_version;
                            response_id = res.response_id;
                            Span::current().record("ttft", started_at.elapsed().as_micros());
                        }
                        for candidate in res.candidates {
                            for part in candidate.content.parts {
                                match part.part {
                                    Part::Text(text) => {
                                        content.push_str(&text);
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
                                        calls.push((
                                            name.to_string(),
                                            args,
                                            part.thought_signature.clone(),
                                        ));
                                    }

                                    x => {
                                        return Err(ModelError::StreamError(format!(
                                            "Unexpected stream part: {x:?}"
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
            let mut parts: Vec<PartWithThought> = vec![];
            if !content.is_empty() {
                parts.push(Part::Text(content).into());
            }

            for (name, args, thought_signature) in &calls {
                parts.push(PartWithThought {
                    part: Part::FunctionCall {
                        name: name.clone(),
                        args: args.clone(),
                    },
                    thought_signature: thought_signature.clone(),
                });
            }

            let response = GenerateContentResponse {
                candidates: vec![Candidate {
                    content: Content {
                        role: Role::Model,
                        parts,
                    },
                    citation_metadata: None,
                    finish_reason: Some(reason.clone()),
                    safety_ratings: None,
                }],
                response_id,
                model_version,
                usage_metadata: usage_metadata.clone(),
            };

            return Ok((reason, calls, usage_metadata, Some(response)));
        }

        tracing::error!("No finish reason found in stream. Last chunk: {last_chunk:?}");

        unreachable!();
    }

    async fn execute_inner(
        &self,
        call: GenerateContentRequest,
        span: Span,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<InnerExecutionResult> {
        let model_name = self.params.model.as_ref().unwrap();
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
        .map_err(|e| LLMError::CustomError(e.to_string()))?;

        let response = async move {
            let result = self.client.invoke(model_name, call).await;
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
                    "raw_usage",
                    JsonValue(&serde_json::to_value(usage)?).as_value(),
                );
                if let Some(usage) = Self::map_usage(Some(&usage.clone())) {
                    span.record("usage", JsonValue(&serde_json::to_value(usage)?).as_value());
                }
            }
            Ok::<_, LLMError>(response)
        }
        .instrument(span.clone().or_current())
        .await?;
        let mut finish_reason = None;
        let mut calls: Vec<(String, HashMap<String, Value>, Option<String>)> = vec![];
        let mut text = String::new();
        for candidate in response.candidates {
            if let Some(reason) = candidate.finish_reason {
                finish_reason = Some(reason);
            }
            for part in candidate.content.parts {
                match part.part {
                    Part::Text(t) => {
                        text.push_str(&t);
                    }
                    Part::FunctionCall { name, args } => {
                        calls.push((name.to_string(), args, part.thought_signature.clone()));
                    }

                    x => {
                        return Err(ModelError::StreamError(format!(
                            "Unexpected stream part: {x:?}"
                        ))
                        .into());
                    }
                };
            }
        }

        if !calls.is_empty() {
            let mut call_messages = vec![];
            for (name, args, thought_signature) in calls.clone() {
                call_messages.push(Content {
                    role: Role::Model,
                    parts: vec![PartWithThought {
                        part: Part::FunctionCall { name, args },
                        thought_signature: thought_signature.clone(),
                    }],
                });
            }

            let tool_calls_str = serde_json::to_string(
                &calls
                    .iter()
                    .enumerate()
                    .map(|(index, c)| ToolCall {
                        index: Some(index),
                        id: c.0.clone(),
                        r#type: "function".to_string(),
                        function: crate::types::gateway::FunctionCall {
                            name: c.0.clone(),
                            arguments: serde_json::to_string(&c.1).unwrap(),
                        },
                        extra_content: c.2.as_ref().map(|s| ToolCallExtra {
                            google: Some(GoogleToolCallExtra {
                                thought_signature: s.clone(),
                            }),
                        }),
                    })
                    .collect::<Vec<_>>(),
            )?;

            let name = map_calls_to_tool_names(&calls);
            let tools_span = tracing::info_span!(
                target: target!(),
                parent: span.clone(),
                events::SPAN_TOOLS,
                tool_calls=tool_calls_str,
                tool.name=name
            );

            let tool = self.tools.get(&calls[0].0);
            if let Some(tool) = tool {
                if tool.stop_at_call() {
                    let usage = response
                        .usage_metadata
                        .as_ref()
                        .map(|u| CompletionModelUsage {
                            input_tokens: u.prompt_token_count as u32,
                            output_tokens: (u.total_token_count - u.prompt_token_count) as u32,
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
                            usage: usage.clone(),
                            finish_reason,
                            tool_calls: calls
                                .iter()
                                .map(|(tool_name, params, thought_signature)| {
                                    Ok(ModelToolCall {
                                        tool_id: tool_name.clone(),
                                        tool_name: tool_name.clone(),
                                        input: serde_json::to_string(params)?,
                                        extra_content: thought_signature.as_ref().map(|s| {
                                            ToolCallExtra {
                                                google: Some(GoogleToolCallExtra {
                                                    thought_signature: s.clone(),
                                                }),
                                            }
                                        }),
                                    })
                                })
                                .collect::<Result<Vec<ModelToolCall>, LLMError>>()?,
                            credentials_ident: self.credentials_ident.clone(),
                        }),
                    )))
                    .await
                    .map_err(|e| LLMError::CustomError(e.to_string()))?;

                    return Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                role: "assistant".to_string(),
                                content: if text.is_empty() {
                                    None
                                } else {
                                    Some(ChatCompletionContent::Text(text.clone()))
                                },
                                tool_calls: Some(
                                    calls
                                        .iter()
                                        .enumerate()
                                        .map(|(index, (tool_name, params, thought_signature))| {
                                            Ok(ToolCall {
                                                index: Some(index),
                                                id: tool_name.clone(),
                                                r#type: "function".to_string(),
                                                function: crate::types::gateway::FunctionCall {
                                                    name: tool_name.clone(),
                                                    arguments: serde_json::to_string(params)?,
                                                },
                                                extra_content: thought_signature.as_ref().map(
                                                    |s| ToolCallExtra {
                                                        google: Some(GoogleToolCallExtra {
                                                            thought_signature: s.clone(),
                                                        }),
                                                    },
                                                ),
                                            })
                                        })
                                        .collect::<Result<Vec<ToolCall>, LLMError>>()?,
                                ),
                                ..Default::default()
                            },
                            ModelFinishReason::ToolCalls,
                            response.response_id,
                            chrono::Utc::now().timestamp() as u32,
                            response.model_version,
                            usage,
                        )
                        .into(),
                    ));
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

            let conversation_messages = [input_messages, call_messages, tools_messages].concat();

            return Ok(InnerExecutionResult::NextCall(conversation_messages));
        }

        match finish_reason {
            Some(FinishReason::Stop) | Some(FinishReason::MaxTokens) => {
                let usage = response
                    .usage_metadata
                    .as_ref()
                    .map(|u| CompletionModelUsage {
                        input_tokens: u.prompt_token_count as u32,
                        output_tokens: (u.total_token_count - u.prompt_token_count) as u32,
                        total_tokens: u.total_token_count as u32,
                        ..Default::default()
                    });

                let finish_reason = Self::map_finish_reason(
                    &finish_reason.expect("Finish reason is already checked"),
                    false,
                );
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
                        usage: usage.clone(),
                        finish_reason: finish_reason.clone(),
                        tool_calls: vec![],
                        credentials_ident: self.credentials_ident.clone(),
                    }),
                )))
                .await
                .map_err(|e| LLMError::CustomError(e.to_string()))?;

                Ok(InnerExecutionResult::Finish(
                    ChatCompletionMessageWithFinishReason::new(
                        ChatCompletionMessage {
                            role: "assistant".to_string(),
                            content: Some(ChatCompletionContent::Text(text)),
                            ..Default::default()
                        },
                        finish_reason,
                        response.response_id,
                        chrono::Utc::now().timestamp() as u32,
                        response.model_version,
                        usage,
                    )
                    .into(),
                ))
            }
            _ => {
                let err = Self::handle_finish_reason(finish_reason);

                Err(err)
            }
        }
    }

    async fn execute(
        &self,
        input_messages: Vec<Content>,
        tx: &tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        let mut gemini_calls = vec![input_messages];
        let mut retries_left = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(call) = gemini_calls.pop() {
            let span = create_model_span!(SPAN_GEMINI, target!("chat"), &tags, retries_left);

            let request = self.build_request(call.clone())?;

            span.record("input", serde_json::to_string(&request)?);
            span.record("request", serde_json::to_string(&request)?);

            let result = self
                .execute_inner(request, span.clone(), tx, tags.clone())
                .await;

            match result.map_err(|e| record_map_err(e, span.clone())) {
                Ok(InnerExecutionResult::Finish(message)) => return Ok(*message),
                Ok(InnerExecutionResult::NextCall(messages)) => {
                    gemini_calls.push(messages);
                    continue;
                }
                Err(e) => {
                    span.record("error", e.to_string());
                    if retries_left == 0 {
                        return Err(e);
                    } else {
                        gemini_calls.push(call);
                    }
                    retries_left -= 1;
                }
            }
        }
        unreachable!();
    }

    fn handle_finish_reason(finish_reason: Option<FinishReason>) -> LLMError {
        ModelError::FinishError(ModelFinishError::Custom(format!("{finish_reason:?}"))).into()
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
            FinishReason::Blocklist => ModelFinishReason::Other("Blocklist".to_string()),
            FinishReason::ProhibitedContent => {
                ModelFinishReason::Other("Prohibited Content".to_string())
            }
            FinishReason::Spii => ModelFinishReason::Other("Spii".to_string()),
            FinishReason::MalformedFunctionCall => {
                ModelFinishReason::Other("Malformed Function Call".to_string())
            }
            FinishReason::ImageSafety => ModelFinishReason::Other("Image Safety".to_string()),
            FinishReason::UnexpectedToolCall => {
                ModelFinishReason::Other("Unexpected Tool Call".to_string())
            }
            FinishReason::TooManyToolCalls => {
                ModelFinishReason::Other("Too Many Tool Calls".to_string())
            }
            FinishReason::Language => ModelFinishReason::Other("Unsupported Language".to_string()),
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

    fn map_tool_call(t: &(String, HashMap<String, Value>, Option<String>)) -> ModelToolCall {
        ModelToolCall {
            tool_id: t.0.clone(),
            tool_name: t.0.clone(),
            input: serde_json::to_string(&t.1).unwrap(),
            extra_content: t.2.as_ref().map(|s| ToolCallExtra {
                google: Some(GoogleToolCallExtra {
                    thought_signature: s.clone(),
                }),
            }),
        }
    }

    async fn execute_stream_inner(
        &self,
        call: GenerateContentRequest,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        call_span: Span,
        tags: HashMap<String, String>,
    ) -> LLMResult<InnerExecutionResult> {
        let model_name = self.params.model.as_ref().unwrap();
        let input_messages = call.contents.clone();
        let started_at = std::time::Instant::now();
        let stream = self.client.stream(model_name, call).await?;
        tokio::pin!(stream);
        tx.send(Some(ModelEvent::new(
            &call_span,
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
        .map_err(|e| LLMError::CustomError(e.to_string()))?;

        let (finish_reason, tool_calls, usage, output) = self
            .process_stream(stream, &tx, started_at)
            .instrument(call_span.clone())
            .await?;

        let trace_finish_reason = Self::map_finish_reason(&finish_reason, !tool_calls.is_empty());
        call_span.record(
            "raw_usage",
            JsonValue(&serde_json::to_value(usage.clone())?).as_value(),
        );

        let usage = Self::map_usage(usage.as_ref());
        if let Some(usage) = &usage {
            call_span.record("usage", JsonValue(&serde_json::to_value(usage)?).as_value());
        }

        tx.send(Some(ModelEvent::new(
            &call_span,
            ModelEventType::LlmStop(LLMFinishEvent {
                provider_name: SPAN_GEMINI.to_string(),
                model_name: self
                    .params
                    .model
                    .clone()
                    .map(|m| m.to_string())
                    .unwrap_or_default(),
                output: None,
                usage: usage.clone(),
                finish_reason: trace_finish_reason.clone(),
                tool_calls: tool_calls.iter().map(Self::map_tool_call).collect(),
                credentials_ident: self.credentials_ident.clone(),
            }),
        )))
        .await
        .map_err(|e| LLMError::CustomError(e.to_string()))?;

        call_span.record("output", serde_json::to_string(&output)?);
        if !tool_calls.is_empty() {
            let mut call_messages = vec![];
            let mut tools = vec![];
            for (index, (name, args, thought_signature)) in tool_calls.clone().iter().enumerate() {
                tools.push(ToolCall {
                    index: Some(index),
                    id: name.clone(),
                    r#type: "function".to_string(),
                    function: crate::types::gateway::FunctionCall {
                        name: name.clone(),
                        arguments: serde_json::to_string(args)?,
                    },
                    extra_content: thought_signature.as_ref().map(|s| ToolCallExtra {
                        google: Some(GoogleToolCallExtra {
                            thought_signature: s.clone(),
                        }),
                    }),
                });
                call_messages.push(Content {
                    role: Role::Model,
                    parts: vec![Part::FunctionCall {
                        name: name.clone(),
                        args: args.clone(),
                    }
                    .into()],
                });
            }
            let tool_calls_str = serde_json::to_string(&tools)?;
            let name = map_calls_to_tool_names(&tool_calls);
            let tools_span = tracing::info_span!(
                target: target!(),
                parent: call_span.id(),
                events::SPAN_TOOLS,
                tool_calls=tool_calls_str,
                tool.name=name
            );
            let tool = self.tools.get(&tool_calls[0].0);
            if let Some(tool) = tool {
                if tool.stop_at_call() {
                    return Ok(InnerExecutionResult::Finish(
                        ChatCompletionMessageWithFinishReason::new(
                            ChatCompletionMessage {
                                ..Default::default()
                            },
                            ModelFinishReason::ToolCalls,
                            output
                                .as_ref()
                                .map(|r| r.response_id.clone())
                                .unwrap_or_default(),
                            chrono::Utc::now().timestamp() as u32,
                            output
                                .as_ref()
                                .map(|r| r.model_version.clone())
                                .unwrap_or_default(),
                            usage,
                        )
                        .into(),
                    ));
                }
            }

            tools_span.follows_from(call_span.id());
            let tool_call_parts =
                Self::handle_tool_calls(tool_calls.iter(), &self.tools, &tx, tags.clone())
                    .instrument(tools_span.clone())
                    .await;
            let tools_messages = vec![Content {
                role: Role::User,
                parts: tool_call_parts,
            }];

            let conversation_messages = [input_messages, call_messages, tools_messages].concat();

            return Ok(InnerExecutionResult::NextCall(conversation_messages));
        }

        match finish_reason {
            FinishReason::Stop | FinishReason::MaxTokens => Ok(InnerExecutionResult::Finish(
                ChatCompletionMessageWithFinishReason::new(
                    ChatCompletionMessage {
                        ..Default::default()
                    },
                    Self::map_finish_reason(&finish_reason, !tool_calls.is_empty()),
                    output
                        .as_ref()
                        .map(|r| r.response_id.clone())
                        .unwrap_or_default(),
                    chrono::Utc::now().timestamp() as u32,
                    output
                        .as_ref()
                        .map(|r| r.model_version.clone())
                        .unwrap_or_default(),
                    usage,
                )
                .into(),
            )),
            other => Err(Self::handle_finish_reason(Some(other))),
        }
    }

    async fn execute_stream(
        &self,
        input_messages: Vec<Content>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        tags: HashMap<String, String>,
    ) -> LLMResult<()> {
        let mut gemini_calls = vec![input_messages];

        let mut retries_left = self
            .execution_options
            .max_retries
            .unwrap_or(DEFAULT_MAX_RETRIES);
        while let Some(call) = gemini_calls.pop() {
            let span = create_model_span!(SPAN_GEMINI, target!("chat"), &tags, retries_left);

            let request = self.build_request(call.clone())?;

            span.record("input", serde_json::to_string(&request)?);
            span.record("request", serde_json::to_string(&request)?);

            let result = self
                .execute_stream_inner(request, tx.clone(), span.clone(), tags.clone())
                .await;

            match result.map_err(|e| record_map_err(e, span.clone())) {
                Ok(InnerExecutionResult::Finish(_)) => return Ok(()),
                Ok(InnerExecutionResult::NextCall(messages)) => {
                    gemini_calls.push(messages);
                    continue;
                }
                Err(e) => {
                    span.record("error", e.to_string());
                    if retries_left == 0 {
                        return Err(e);
                    } else {
                        gemini_calls.push(call);
                    }
                    retries_left -= 1;
                }
            }
        }

        Ok(())
    }

    fn map_previous_messages(
        messages_dto: Vec<Message>,
        input_variables: HashMap<String, Value>,
    ) -> LLMResult<Vec<Content>> {
        // convert serde::Map into HashMap
        let mut messages = vec![];
        let mut tool_results_remaining = 0;
        let mut tool_calls_collected = vec![];
        for m in messages_dto.iter() {
            let request_message = {
                match m.r#type {
                    MessageType::SystemMessage => Some(Content::user(render(
                        m.content.clone().unwrap_or_default(),
                        &input_variables,
                    ))),

                    MessageType::AIMessage => {
                        if let Some(tool_calls) = &m.tool_calls {
                            tool_results_remaining = tool_calls.len();
                            tool_calls_collected = vec![];
                            Some(Content {
                                role: Role::Model,
                                parts: tool_calls
                                    .iter()
                                    .map(|c| {
                                        let args = if c.function.arguments.is_empty() {
                                            "{}"
                                        } else {
                                            &c.function.arguments
                                        };
                                        Ok(PartWithThought {
                                            part: Part::FunctionCall {
                                                name: c.id.clone(),
                                                args: serde_json::from_str(args)?,
                                            },
                                            thought_signature: c.extra_content.as_ref().and_then(
                                                |e| {
                                                    e.google
                                                        .as_ref()
                                                        .map(|g| g.thought_signature.clone())
                                                },
                                            ),
                                        })
                                    })
                                    .collect::<Result<Vec<PartWithThought>, LLMError>>()?,
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
                        tool_calls_collected.push(
                            Part::FunctionResponse {
                                name: m.tool_call_id.clone().unwrap_or_default(),
                                response: Some(PartFunctionResponse {
                                    fields: HashMap::from([(
                                        "content
                                        "
                                        .to_string(),
                                        content,
                                    )]),
                                }),
                            }
                            .into(),
                        );
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
    ) -> LLMResult<ChatCompletionMessageWithFinishReason> {
        let conversational_messages =
            self.construct_messages(input_variables, previous_messages)?;
        self.execute(conversational_messages, &tx, tags).await
    }

    async fn stream(
        &self,
        input_variables: HashMap<String, Value>,
        tx: tokio::sync::mpsc::Sender<Option<ModelEvent>>,
        previous_messages: Vec<Message>,
        tags: HashMap<String, String>,
    ) -> LLMResult<()> {
        let conversational_messages =
            self.construct_messages(input_variables, previous_messages)?;
        self.execute_stream(conversational_messages, tx, tags).await
    }
}

impl GeminiModel {
    fn construct_messages(
        &self,
        input_variables: HashMap<String, Value>,
        previous_messages: Vec<Message>,
    ) -> LLMResult<Vec<Content>> {
        let mut conversational_messages = vec![];
        let previous_messages = Self::map_previous_messages(previous_messages, input_variables)?;
        conversational_messages.extend(previous_messages);

        Ok(conversational_messages)
    }
}

fn construct_user_message(m: &InnerMessage) -> Content {
    match m {
        InnerMessage::Text(text) => Content::user(text.to_string()),
        InnerMessage::Array(content_array) => {
            let mut parts = vec![];
            for m in content_array {
                let msg: Part = match m.r#type {
                    MessageContentType::Text => Part::Text(m.value.clone()),
                    MessageContentType::ImageUrl => {
                        let url = m.value.clone();
                        let base64_data = url
                            .split_once(',')
                            .map_or_else(|| url.as_str(), |(_, data)| data);
                        Part::InlineData {
                            mime_type: "image/png".to_string(),
                            data: base64_data.to_string(),
                        }
                    }
                    MessageContentType::InputAudio => {
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
                parts.push(msg.into())
            }
            Content {
                role: Role::User,
                parts,
            }
        }
    }
}

pub fn record_map_err(e: impl Into<LLMError> + ToString, span: tracing::Span) -> LLMError {
    span.record("error", e.to_string());
    e.into()
}

fn replace_refs_with_defs(schema: Value) -> Value {
    // If schema isn't an object, return as is
    if !schema.is_object() {
        return schema;
    }

    // Clone schema to avoid ownership issues
    let mut result = schema.clone();

    // Extract $defs if they exist
    let defs = if let Some(defs_obj) = result.get("$defs") {
        if defs_obj.is_object() {
            defs_obj.clone()
        } else {
            serde_json::json!({})
        }
    } else {
        serde_json::json!({})
    };

    // Remove $defs from result
    if let Some(obj) = result.as_object_mut() {
        obj.remove("$defs");
    }

    // Function to recursively replace $ref
    fn replace_refs(value: &mut Value, defs: &Value) {
        match value {
            Value::Object(obj) => {
                // Check if this object has a $ref
                if let Some(ref_val) = obj.get("$ref") {
                    if let Some(ref_str) = ref_val.as_str() {
                        // Extract the definition name from the $ref string
                        // Example: "#/$defs/SubClass" -> "SubClass"
                        if let Some(def_name) = ref_str.strip_prefix("#/$defs/") {
                            // Replace object with the referenced definition
                            if let Some(def) = defs.get(def_name) {
                                // Deep clone the definition to avoid ownership issues
                                let mut def_clone = def.clone();
                                // Recursively replace any refs in the definition
                                replace_refs(&mut def_clone, defs);
                                *value = def_clone;
                                return;
                            }
                        }
                    }
                }

                // Process all properties in this object
                for (_, v) in obj.iter_mut() {
                    replace_refs(v, defs);
                }
            }
            Value::Array(arr) => {
                // Process all items in the array
                for item in arr.iter_mut() {
                    replace_refs(item, defs);
                }
            }
            _ => {} // Primitive values don't need processing
        }
    }

    // Start the recursive replacement
    replace_refs(&mut result, &defs);
    result
}

/// Removes all additionalProperties fields from a JSON schema
fn remove_additional_properties(schema: Value) -> Value {
    // If schema isn't an object, return as is
    if !schema.is_object() {
        return schema;
    }

    // Clone schema to avoid ownership issues
    let mut result = schema.clone();

    // Function to recursively remove additionalProperties
    fn remove_props(value: &mut Value) {
        match value {
            Value::Object(obj) => {
                // Remove additionalProperties from this object
                obj.remove("additionalProperties");

                // Process all properties in this object
                for (_, v) in obj.iter_mut() {
                    remove_props(v);
                }
            }
            Value::Array(arr) => {
                // Process all items in the array
                for item in arr.iter_mut() {
                    remove_props(item);
                }
            }
            _ => {} // Primitive values don't need processing
        }
    }

    // Start the recursive removal
    remove_props(&mut result);
    result
}

/// Normalizes nullable types in JSON schema
/// When `anyOf` or `oneOf` contains `{"type": "null"}`, this function:
/// 1. Removes the null type entry
/// 2. Adds `nullable: true` to the remaining types
/// 3. If only one type remains, it removes the anyOf/oneOf wrapper
fn normalize_nullable_types(schema: Value) -> Value {
    // If schema isn't an object or array, return as is
    if !schema.is_object() && !schema.is_array() {
        return schema;
    }

    // Clone schema to avoid ownership issues
    let mut result = schema.clone();

    // Function to recursively normalize nullable types
    fn normalize(value: &mut Value) {
        match value {
            Value::Object(obj) => {
                // Check if this object has anyOf or oneOf arrays
                for type_key in ["anyOf", "oneOf"].iter() {
                    if let Some(Value::Array(types_arr)) = obj.get_mut(*type_key) {
                        // Look for {type: null} entry
                        let mut has_null_type = false;
                        let mut null_index = None;

                        for (i, item) in types_arr.iter().enumerate() {
                            if let Value::Object(item_obj) = item {
                                if let Some(item_type) = item_obj.get("type") {
                                    if item_type.as_str() == Some("null") {
                                        has_null_type = true;
                                        null_index = Some(i);
                                        break;
                                    }
                                }
                            }
                        }

                        if has_null_type {
                            // Remove the null type entry
                            if let Some(idx) = null_index {
                                types_arr.remove(idx);
                            }

                            // Add nullable: true to all other entries
                            for item in types_arr.iter_mut() {
                                if let Value::Object(item_obj) = item {
                                    item_obj.insert("nullable".to_string(), Value::Bool(true));
                                }
                            }

                            // If only one type remains, replace the anyOf/oneOf with it
                            if types_arr.len() == 1 {
                                let single_type = types_arr.remove(0);
                                obj.remove(*type_key);
                                if let Value::Object(single_obj) = single_type {
                                    for (k, v) in single_obj {
                                        obj.insert(k, v);
                                    }
                                }
                            }
                        }
                    }
                }

                // Recursively process all properties in this object
                for (_, v) in obj.iter_mut() {
                    normalize(v);
                }
            }
            Value::Array(arr) => {
                // Process all items in the array
                for item in arr.iter_mut() {
                    normalize(item);
                }
            }
            _ => {} // Primitive values don't need processing
        }
    }

    // Start the recursive normalization
    normalize(&mut result);
    result
}
