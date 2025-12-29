use serde::{Deserialize, Serialize};
use vllora_llm::async_openai::types::chat::ChatCompletionMessageToolCalls;
use vllora_llm::async_openai::types::chat::CreateChatCompletionRequest;
use vllora_llm::async_openai::types::chat::CreateChatCompletionResponse;
use vllora_llm::clust::messages::{
    Content as ClustContent, ContentBlock, MessagesRequestBody, MessagesResponseBody,
    Role as ClustRole,
};
use vllora_llm::provider::gemini::types::{
    GenerateContentRequest, GenerateContentResponse, Part, Role as GeminiRole,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Request {
    Openai(Box<CreateChatCompletionRequest>),
    Anthropic(MessagesRequestBody),
    // Bedrock(BedrockRequest), todo: check if this is needed
    Gemini(GenerateContentRequest),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Response {
    Openai(CreateChatCompletionResponse),
    Anthropic(MessagesResponseBody),
    Gemini(GenerateContentResponse),
}

#[derive(Debug)]
pub enum Message {
    // Role, Content
    Text(String, String),
    ToolCall {
        name: String,
        id: Option<String>,
        arguments: serde_json::Value,
    },
}

impl From<ChatCompletionMessageToolCalls> for Message {
    fn from(val: ChatCompletionMessageToolCalls) -> Self {
        match val {
            ChatCompletionMessageToolCalls::Function(function) => Message::ToolCall {
                name: function.function.name.clone(),
                id: Some(function.id.clone()),
                arguments: serde_json::to_value(&function.function.arguments).unwrap_or_default(),
            },
            ChatCompletionMessageToolCalls::Custom(custom) => Message::ToolCall {
                name: custom.custom_tool.name.clone(),
                id: Some(custom.id.clone()),
                arguments: serde_json::to_value(&custom.custom_tool.input).unwrap_or_default(),
            },
        }
    }
}

impl From<&ChatCompletionMessageToolCalls> for Message {
    fn from(val: &ChatCompletionMessageToolCalls) -> Self {
        val.clone().into()
    }
}

pub fn map_request(request: &serde_json::Value) -> Result<Request, serde_json::Error> {
    if let serde_json::Value::String(obj) = request {
        serde_json::from_str(obj)
    } else {
        serde_json::from_value(request.clone())
    }
}

pub fn map_response(response: &serde_json::Value) -> Result<Response, serde_json::Error> {
    if let serde_json::Value::String(obj) = response {
        serde_json::from_str(obj)
    } else {
        serde_json::from_value(response.clone())
    }
}

pub fn generate_response_messages(response: &Response) -> Vec<Message> {
    let mut result = Vec::new();

    match response {
        Response::Openai(openai_resp) => {
            // OpenAI responses have choices array
            for choice in &openai_resp.choices {
                let msg = &choice.message;

                // Handle text content
                if let Some(content) = &msg.content {
                    result.push(Message::Text("assistant".to_string(), content.clone()));
                }

                // Handle tool calls
                if let Some(tool_calls) = &msg.tool_calls {
                    for tool_call in tool_calls {
                        result.push(tool_call.into());
                    }
                }
            }
        }
        Response::Anthropic(anthropic_resp) => {
            // Anthropic responses have content field
            match &anthropic_resp.content {
                ClustContent::SingleText(text) => {
                    result.push(Message::Text("assistant".to_string(), text.clone()));
                }
                ClustContent::MultipleBlocks(blocks) => {
                    let mut texts = Vec::new();
                    for block in blocks {
                        match block {
                            ContentBlock::Text(text_block) => {
                                texts.push(text_block.text.clone());
                            }
                            ContentBlock::ToolUse(tool_use_block) => {
                                // Handle tool calls
                                let tool_use = &tool_use_block.tool_use;
                                result.push(Message::ToolCall {
                                    name: tool_use.name.clone(),
                                    id: Some(tool_use.id.clone()),
                                    arguments: serde_json::to_value(&tool_use.input)
                                        .unwrap_or_else(|_| serde_json::Value::Null),
                                });
                            }
                            _ => {
                                // For other block types, serialize to JSON
                                if let Ok(json_str) = serde_json::to_string(block) {
                                    texts.push(json_str);
                                }
                            }
                        }
                    }
                    if !texts.is_empty() {
                        result.push(Message::Text("assistant".to_string(), texts.join("\n")));
                    }
                }
            }
        }
        Response::Gemini(gemini_resp) => {
            // Gemini responses have candidates array
            for candidate in &gemini_resp.candidates {
                let content = &candidate.content;
                let role = match content.role {
                    GeminiRole::User => "user",
                    GeminiRole::Model => "assistant",
                };

                let mut texts = Vec::new();
                for part_with_thought in &content.parts {
                    match &part_with_thought.part {
                        Part::Text(text) => {
                            texts.push(text.clone());
                        }
                        Part::FunctionCall { name, args } => {
                            result.push(Message::ToolCall {
                                name: name.clone(),
                                id: None,
                                arguments: serde_json::to_value(args)
                                    .unwrap_or_else(|_| serde_json::Value::Null),
                            });
                        }
                        Part::FunctionResponse { name, response } => {
                            let response_str = match response {
                                Some(resp) => serde_json::to_string(&resp.fields)
                                    .unwrap_or_else(|_| "null".to_string()),
                                None => "null".to_string(),
                            };
                            result.push(Message::Text(
                                "tool".to_string(),
                                format!("{}: {}", name, response_str),
                            ));
                        }
                        _ => {
                            // For other part types (InlineData, FileData), serialize to JSON
                            if let Ok(json_str) = serde_json::to_string(&part_with_thought.part) {
                                texts.push(json_str);
                            }
                        }
                    }
                }

                if !texts.is_empty() {
                    result.push(Message::Text(role.to_string(), texts.join("\n")));
                }
            }
        }
    }

    result
}

pub fn generate_messages(request: &Request) -> Vec<Message> {
    use vllora_llm::async_openai::types::chat::{
        ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestDeveloperMessageContent,
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageContent,
        ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessageContent,
    };

    let mut result = Vec::new();

    match request {
        Request::Openai(openai_req) => {
            for msg in &openai_req.messages {
                match msg {
                    ChatCompletionRequestMessage::System(sys_msg) => {
                        let content = match &sys_msg.content {
                            ChatCompletionRequestSystemMessageContent::Text(text) => text.clone(),
                            ChatCompletionRequestSystemMessageContent::Array(_) => {
                                // For array content, serialize to JSON string
                                serde_json::to_string(&sys_msg.content).unwrap_or_default()
                            }
                        };
                        result.push(Message::Text("system".to_string(), content));
                    }
                    ChatCompletionRequestMessage::User(user_msg) => {
                        let content = match &user_msg.content {
                            ChatCompletionRequestUserMessageContent::Text(text) => text.clone(),
                            ChatCompletionRequestUserMessageContent::Array(_) => {
                                serde_json::to_string(&user_msg.content).unwrap_or_default()
                            }
                        };
                        result.push(Message::Text("user".to_string(), content));
                    }
                    ChatCompletionRequestMessage::Assistant(assistant_msg) => {
                        // Handle text content
                        if let Some(content) = &assistant_msg.content {
                            let text_content = match content {
                                ChatCompletionRequestAssistantMessageContent::Text(text) => {
                                    text.clone()
                                }
                                ChatCompletionRequestAssistantMessageContent::Array(_) => {
                                    serde_json::to_string(content).unwrap_or_default()
                                }
                            };
                            result.push(Message::Text("assistant".to_string(), text_content));
                        }

                        // Handle tool calls
                        if let Some(tool_calls) = &assistant_msg.tool_calls {
                            for tool_call in tool_calls {
                                result.push(tool_call.into());
                            }
                        }
                    }
                    ChatCompletionRequestMessage::Tool(tool_msg) => {
                        let content = match &tool_msg.content {
                            ChatCompletionRequestToolMessageContent::Text(text) => text.clone(),
                            ChatCompletionRequestToolMessageContent::Array(_) => {
                                serde_json::to_string(&tool_msg.content).unwrap_or_default()
                            }
                        };
                        result.push(Message::Text("tool".to_string(), content));
                    }
                    ChatCompletionRequestMessage::Developer(dev_msg) => {
                        let content = match &dev_msg.content {
                            ChatCompletionRequestDeveloperMessageContent::Text(text) => {
                                text.clone()
                            }
                            ChatCompletionRequestDeveloperMessageContent::Array(_) => {
                                serde_json::to_string(&dev_msg.content).unwrap_or_default()
                            }
                        };
                        result.push(Message::Text("developer".to_string(), content));
                    }
                    ChatCompletionRequestMessage::Function(func_msg) => {
                        // Function messages are typically used for function calling
                        let content = serde_json::to_string(func_msg).unwrap_or_default();
                        result.push(Message::Text("function".to_string(), content));
                    }
                }
            }
        }
        Request::Anthropic(anthropic_req) => {
            for msg in &anthropic_req.messages {
                let role = match msg.role {
                    ClustRole::User => "user",
                    ClustRole::Assistant => "assistant",
                };

                let content = match &msg.content {
                    ClustContent::SingleText(text) => text.clone(),
                    ClustContent::MultipleBlocks(blocks) => {
                        // Extract text from blocks
                        let mut texts = Vec::new();
                        for block in blocks {
                            match block {
                                ContentBlock::Text(text_block) => {
                                    texts.push(text_block.text.clone());
                                }
                                ContentBlock::ToolUse(tool_use_block) => {
                                    // Handle tool calls - ToolUseContentBlock has a .tool_use field
                                    let tool_use = &tool_use_block.tool_use;
                                    result.push(Message::ToolCall {
                                        name: tool_use.name.clone(),
                                        id: Some(tool_use.id.clone()),
                                        arguments: serde_json::to_value(&tool_use.input)
                                            .unwrap_or_default(),
                                    });
                                }
                                ContentBlock::ToolResult(tool_result_block) => {
                                    // ToolResultContentBlock wraps a ToolResult
                                    let tool_result = &tool_result_block.tool_result;
                                    // Convert ToolResultContent to string
                                    let result_content = serde_json::to_string(
                                        &tool_result.content,
                                    )
                                    .unwrap_or_else(|_| format!("{:?}", tool_result.content));
                                    result.push(Message::Text("tool".to_string(), result_content));
                                }
                                _ => {}
                            }
                        }
                        texts.join("\n")
                    }
                };

                if !content.is_empty() {
                    result.push(Message::Text(role.to_string(), content));
                }
            }
        }
        Request::Gemini(gemini_req) => {
            for content in &gemini_req.contents {
                let role = match content.role {
                    GeminiRole::User => "user",
                    GeminiRole::Model => "assistant",
                };

                let mut texts = Vec::new();
                for part_with_thought in &content.parts {
                    match &part_with_thought.part {
                        Part::Text(text) => {
                            texts.push(text.clone());
                        }
                        Part::FunctionCall { name, args } => {
                            result.push(Message::ToolCall {
                                name: name.clone(),
                                id: None,
                                arguments: serde_json::to_value(args).unwrap_or_default(),
                            });
                        }
                        Part::FunctionResponse { name, response } => {
                            let response_str = match response {
                                Some(resp) => {
                                    serde_json::to_string(&resp.fields).unwrap_or_default()
                                }
                                None => "null".to_string(),
                            };
                            result.push(Message::Text(
                                "tool".to_string(),
                                format!("{}: {}", name, response_str),
                            ));
                        }
                        _ => {
                            // For other part types (InlineData, FileData), serialize to JSON
                            if let Ok(json_str) = serde_json::to_string(&part_with_thought.part) {
                                texts.push(json_str);
                            }
                        }
                    }
                }

                if !texts.is_empty() {
                    result.push(Message::Text(role.to_string(), texts.join("\n")));
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use crate::mcp::server::shared::*;

    #[test]
    fn test_map_request() {
        // Create a valid Gemini request JSON string
        let json_str = r#"{"contents":[{"role":"user","parts":[{"text":"what is my latest trace?"}]}],"generation_config":{"maxOutputTokens":null,"temperature":null,"topP":null,"topK":null,"stopSequences":null,"candidateCount":null,"presencePenalty":null,"frequencyPenalty":null,"seed":null,"responseLogprobs":null,"logprobs":null,"responseMimeType":null,"responseSchema":null},"tools":[{"function_declarations":[{"name":"search_traces","description":"Search traces for analysis","parameters":{"type":"object","properties":{"sort":{"description":"Sorting configuration for the result set."},"filters":{"description":"Additional filters to narrow down traces."},"page":{"description":"Pagination configuration for the result set."},"time_range":{"description":"Time range configuration for the search."},"include":{"description":"Flags to control which extra data is included per trace."}},"required":[]}}]}]}"#;
        let raw_request = serde_json::Value::String(json_str.to_string());
        let mapped_request = map_request(&raw_request);
        println!("mapped_request: {:#?}", mapped_request);

        // Verify it's a Gemini request
        match mapped_request {
            Ok(Request::Gemini(_)) => {}
            _ => panic!("Expected Gemini request"),
        }
    }
}
