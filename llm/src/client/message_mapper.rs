use crate::types::gateway::{ChatCompletionContent, ChatCompletionMessage, ContentType};
use crate::types::message::AudioDetail;
use crate::types::message::AudioFormat;
use crate::types::message::Message;
use crate::types::message::MessageContentPart;
use crate::types::message::MessageContentPartOptions;
use crate::types::message::MessageContentType;
use crate::types::message::MessageType;

#[derive(Debug, thiserror::Error)]
pub enum MessageMapperError {
    #[error("System message can only have one content")]
    SystemMessageCanOnlyHaveOneContent,
    #[error("System message content is empty")]
    SystemMessageContentIsEmpty,
    #[error("Image url are not supported for system messages")]
    ImageUrlNotSupportedForSystemMessages,
    #[error("Input audio are not supported for system messages")]
    InputAudioNotSupportedForSystemMessages,
    #[error("Audio data is empty")]
    AudioDataIsEmpty,
    #[error("Unsupported audio format: {0}")]
    UnsupportedAudioFormat(String),
}

pub struct MessageMapper {}

impl MessageMapper {
    pub fn map_completions_message_to_vllora_message(
        message: &ChatCompletionMessage,
        model_name: &str,
        user: &str,
    ) -> Result<Message, MessageMapperError> {
        let content = if let Some(content) = &message.content {
            match content {
                ChatCompletionContent::Text(content) => {
                    if message.cache_control.is_none() {
                        Some(content.clone())
                    } else {
                        None
                    }
                }
                ChatCompletionContent::Content(_) => None,
            }
        } else {
            None
        };

        let content_array = if let Some(content) = &message.content {
            match content {
                ChatCompletionContent::Text(content) => {
                    if message.cache_control.is_none() {
                        Ok(vec![])
                    } else {
                        Ok(vec![MessageContentPart {
                            r#type: MessageContentType::Text,
                            value: content.clone(),
                            additional_options: None,
                            cache_control: message.cache_control.clone(),
                        }])
                    }
                }
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
                                let audio = c
                                    .audio
                                    .as_ref()
                                    .ok_or(MessageMapperError::AudioDataIsEmpty)?;
                                MessageContentPart {
                                    r#type: MessageContentType::InputAudio,
                                    value: audio.data.clone(),
                                    additional_options: Some(MessageContentPartOptions::Audio(
                                        AudioDetail {
                                            r#type: match audio.format.as_str() {
                                                "mp3" => AudioFormat::Mp3,
                                                "wav" => AudioFormat::Wav,
                                                f => {
                                                    return Err(
                                                        MessageMapperError::UnsupportedAudioFormat(
                                                            format!("Unsupported audio format {f}"),
                                                        ),
                                                    );
                                                }
                                            },
                                        },
                                    )),
                                    cache_control: c.cache_control.clone(),
                                }
                            }
                        })
                    })
                    .collect::<Result<Vec<MessageContentPart>, MessageMapperError>>(),
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
            created_at: None,
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
