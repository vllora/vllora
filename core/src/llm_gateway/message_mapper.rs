use std::collections::HashSet;

use crate::types::{
    gateway::{ChatCompletionContent, ChatCompletionMessage, ContentType},
    message::{MessageType, PromptMessage},
    threads::{
        AudioDetail, AudioFormat, Message, MessageContentPart, MessageContentPartOptions,
        MessageContentType,
    },
};

use crate::GatewayError;

pub struct MessageMapper {}

impl MessageMapper {
    pub fn map_prompt_message(
        messages: &[ChatCompletionMessage],
    ) -> Result<Vec<PromptMessage>, GatewayError> {
        let mut prompt_messages = vec![];
        for message in messages.iter() {
            if message.role.as_str() == "system" {
                let msg = match &message.content {
                    Some(content) => match content {
                        ChatCompletionContent::Text(content) => content.clone(),
                        ChatCompletionContent::Content(content) => {
                            if content.len() > 1 {
                                return Err(GatewayError::CustomError(
                                    "System message can only have one content".to_string(),
                                ));
                            }

                            let content = content.first().ok_or(GatewayError::CustomError(
                                "System message content is empty".to_string(),
                            ))?;

                            match content.r#type {
                                ContentType::Text => match &content.text {
                                    Some(content) => content.clone(),
                                    None => {
                                        return Err(GatewayError::CustomError(
                                            "System message content is empty".to_string(),
                                        ))
                                    }
                                },
                                ContentType::ImageUrl => {
                                    return Err(GatewayError::CustomError(
                                        "Image url are not supported for system messages"
                                            .to_string(),
                                    ))
                                }
                                ContentType::InputAudio => {
                                    return Err(GatewayError::CustomError(
                                        "Input audio are not supported for system messages"
                                            .to_string(),
                                    ))
                                }
                            }
                        }
                    },
                    None => {
                        return Err(GatewayError::CustomError(
                            "System message content is empty".to_string(),
                        ))
                    }
                };

                let m = PromptMessage {
                    r#type: MessageType::SystemMessage,
                    msg,
                    wired: false,
                    parameters: HashSet::new(),
                };
                prompt_messages.push(m);
            }
        }

        Ok(prompt_messages)
    }

    pub fn map_completions_message_to_langdb_message(
        message: &ChatCompletionMessage,
        model_name: &str,
        user: &str,
    ) -> Result<Message, GatewayError> {
        let content = if let Some(content) = &message.content {
            match content {
                ChatCompletionContent::Text(content) => Some(content.clone()),
                ChatCompletionContent::Content(_) => None,
            }
        } else {
            None
        };

        let content_array = if let Some(content) = &message.content {
            match content {
                ChatCompletionContent::Text(_) => Ok(vec![]),
                ChatCompletionContent::Content(content) => content
                    .iter()
                    .map(|c| {
                        Ok(match c.r#type {
                            ContentType::Text => MessageContentPart {
                                r#type: MessageContentType::Text,
                                value: c.text.clone().unwrap_or("".to_string()),
                                additional_options: None,
                                cache_control: c.cache_control.clone(),
                            },
                            ContentType::ImageUrl => MessageContentPart {
                                r#type: MessageContentType::ImageUrl,
                                value: c
                                    .image_url
                                    .clone()
                                    .map(|url| url.url.clone())
                                    .unwrap_or("".to_string()),
                                additional_options: None,
                                cache_control: c.cache_control.clone(),
                            },
                            ContentType::InputAudio => {
                                let audio = c.audio.as_ref().ok_or(GatewayError::CustomError(
                                    "Audio data is empty".to_string(),
                                ))?;
                                MessageContentPart {
                                    r#type: MessageContentType::InputAudio,
                                    value: audio.data.clone(),
                                    additional_options: Some(MessageContentPartOptions::Audio(
                                        AudioDetail {
                                            r#type: match audio.format.as_str() {
                                                "mp3" => AudioFormat::Mp3,
                                                "wav" => AudioFormat::Wav,
                                                f => {
                                                    return Err(GatewayError::CustomError(format!(
                                                        "Unsupported audio format {f}"
                                                    )))
                                                }
                                            },
                                        },
                                    )),
                                    cache_control: c.cache_control.clone(),
                                }
                            }
                        })
                    })
                    .collect::<Result<Vec<MessageContentPart>, GatewayError>>(),
            }
        } else {
            Ok(vec![])
        };

        Ok(Message {
            model_name: model_name.to_string(),
            thread_id: None,
            user_id: user.to_string(),
            content_type: MessageContentType::Text,
            content: content.clone(),
            content_array: content_array?,
            r#type: Self::map_role_to_message_type(message.role.as_str()),
            tool_calls: message.tool_calls.clone(),
            tool_call_id: message.tool_call_id.clone(),
        })
    }

    pub fn map_role_to_message_type(role: &str) -> MessageType {
        match role {
            "system" => MessageType::SystemMessage,
            "assistant" | "ai" => MessageType::AIMessage,
            "user" => MessageType::HumanMessage,
            "tool" => MessageType::ToolResult,
            _ => MessageType::HumanMessage,
        }
    }
}
