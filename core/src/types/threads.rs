use std::fmt::Display;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_tuple::{Deserialize_tuple, Serialize_tuple};
use serde_with::serde_as;

use super::{gateway::ToolCall, message::MessageType};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageThread {
    pub id: String,         // UUID
    pub model_name: String, // Corresponding LangDB model
    pub user_id: String,    // UUID
    pub project_id: String, // Project identifier
    pub is_public: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PublicMessageThread {
    pub id: String,         // UUID
    pub is_public: bool,
    pub tenant_id: String,
}

#[serde_as]
#[derive(Serialize, Debug, Clone)]
pub struct Message {
    pub model_name: String,        // Corresponding LangDB model
    pub thread_id: Option<String>, // Identifier of the thread to which this message belongs
    pub user_id: String,           // UUID
    pub content_type: MessageContentType,
    pub content: Option<String>,
    pub content_array: Vec<MessageContentPart>,
    pub r#type: MessageType, // Human / AI Message
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            model_name: String,
            thread_id: Option<String>,
            user_id: String,
            content_type: MessageContentType,
            content: Option<String>,
            content_array: Vec<MessageContentPart>,
            r#type: MessageType,
            tool_call_id: Option<String>,
            tool_calls: Option<serde_json::Value>,
        }

        let helper = Helper::deserialize(deserializer)?;

        let tool_calls = match helper.tool_calls {
            Some(Value::String(s)) => serde_json::from_str(&s).map_err(serde::de::Error::custom)?,
            Some(Value::Array(_)) => helper.tool_calls,
            _ => None,
        };

        Ok(Message {
            model_name: helper.model_name,
            thread_id: helper.thread_id,
            user_id: helper.user_id,
            content_type: helper.content_type,
            content: helper.content,
            content_array: helper.content_array,
            r#type: helper.r#type,
            tool_call_id: helper.tool_call_id,
            tool_calls: tool_calls.and_then(|v| serde_json::from_value(v).ok()),
        })
    }
}

// Value is deserialized into this object selectively
// by a prompt
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum InnerMessage {
    Text(String),
    Array(Vec<MessageContentPart>),
}

impl From<Message> for InnerMessage {
    fn from(val: Message) -> Self {
        match val.content_array.len() {
            0 => InnerMessage::Text(val.content.unwrap_or_default()),
            _ => InnerMessage::Array(val.content_array),
        }
    }
}

#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone)]
pub struct MessageContentPart {
    pub r#type: MessageContentType,
    pub value: String,
    pub additional_options: Option<MessageContentPartOptions>,
}

impl From<MessageContentPart> for Value {
    fn from(val: MessageContentPart) -> Self {
        Value::Array(vec![
            val.r#type.to_string().into(),
            val.value.into(),
            Value::Null,
            // self.additional_options.map_or("".to_string(), |m| {
            //     serde_json::to_string(&m).unwrap_or_default()
            // }),
        ])
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub enum MessageContentType {
    #[default]
    Text,
    ImageUrl,
    InputAudio,
}

impl Display for MessageContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageContentType::Text => f.write_str("Text"),
            MessageContentType::ImageUrl => f.write_str("ImageUrl"),
            MessageContentType::InputAudio => f.write_str("InputAudio"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum MessageContentValue {
    Text(String),
    ImageUrl(Vec<MessageContentPart>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum MessageContentPartOptions {
    Image(ImageDetail),
    Audio(AudioDetail),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AudioDetail {
    pub r#type: AudioFormat,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AudioFormat {
    Mp3,
    Wav,
}
impl MessageContentPartOptions {
    pub fn as_image(&self) -> Option<ImageDetail> {
        match self {
            MessageContentPartOptions::Image(image) => Some(image.to_owned()),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ImageDetail {
    Auto,
    Low,
    High,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageRequest {
    pub model_name: String,
    pub thread_id: Option<String>,
    pub user_id: String,
    pub parameters: IndexMap<String, serde_json::Value>,
    pub message: InnerMessage,
    #[serde(default = "default_include_history")]
    pub include_history: bool,
    #[serde(default)]
    pub history_length: Option<u32>,
}

pub fn default_include_history() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::types::threads::MessageContentPart;

    #[test]
    fn message_serialization() {
        let test = vec![
            MessageContentPart {
                r#type: super::MessageContentType::ImageUrl,
                value: "image/base64".to_string(),
                additional_options: None,
            },
            MessageContentPart {
                r#type: super::MessageContentType::Text,
                value: "How is my image".to_string(),
                additional_options: None,
            },
        ];

        let str2 = serde_json::to_value(&test).unwrap();
        println!("{}", serde_json::to_string_pretty(&test).unwrap());
        assert_eq!(
            str2,
            json!([
                ["ImageUrl", "image/base64", null],
                ["Text", "How is my image", null]
            ])
        );
    }
}
