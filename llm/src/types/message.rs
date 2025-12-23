use crate::types::gateway::{CacheControl, File};
use crate::types::ToolCall;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_tuple::Serialize_tuple;
use serde_with::serde_as;
use std::fmt::Display;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum MessageType {
    #[serde(rename = "system")]
    SystemMessage,
    #[serde(rename = "ai", alias = "assistant")]
    AIMessage,
    #[serde(rename = "human")]
    HumanMessage,
    #[serde(rename = "tool")]
    ToolResult,
}

impl Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageType::SystemMessage => f.write_str("system"),
            MessageType::AIMessage => f.write_str("ai"),
            MessageType::HumanMessage => f.write_str("human"),
            MessageType::ToolResult => f.write_str("tool"),
        }
    }
}

impl FromStr for MessageType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "system" => Ok(MessageType::SystemMessage),
            "ai" | "assistant" => Ok(MessageType::AIMessage),
            "human" => Ok(MessageType::HumanMessage),
            "tool" => Ok(MessageType::ToolResult),
            _ => Err(format!("Invalid message type: {}", s)),
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

impl Message {
    pub fn is_content_identical(&self, other: &Message) -> bool {
        self.content == other.content
            && self.content_array == other.content_array
            && self.content_type == other.content_type
            && self.r#type == other.r#type
            && self.tool_call_id == other.tool_call_id
            && self.tool_calls == other.tool_calls
    }
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
            created_at: None,
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

#[derive(Serialize_tuple, Debug, Clone, PartialEq)]
pub struct MessageContentPart {
    pub r#type: MessageContentType,
    pub value: String,
    pub additional_options: Option<MessageContentPartOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    pub file: Option<File>,
}

impl<'de> Deserialize<'de> for MessageContentPart {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Use a custom deserializer that can handle both 3-tuple and 4-tuple formats
        struct MessageContentPartVisitor;

        impl<'de> serde::de::Visitor<'de> for MessageContentPartVisitor {
            type Value = MessageContentPart;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a tuple of 3 or 4 elements")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let r#type: MessageContentType = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;

                let value: String = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;

                let additional_options: Option<MessageContentPartOptions> = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?;

                // Try to get the fourth element (cache_control), but it's optional
                let cache_control: Option<CacheControl> = match seq.next_element()? {
                    Some(serde_json::Value::Null) => None,
                    Some(value) => serde_json::from_value(value).ok(),
                    None => None,
                };

                Ok(MessageContentPart {
                    r#type,
                    value,
                    additional_options,
                    cache_control,
                    file: None,
                })
            }
        }

        deserializer.deserialize_seq(MessageContentPartVisitor)
    }
}

impl From<MessageContentPart> for Value {
    fn from(val: MessageContentPart) -> Self {
        Value::Array(vec![
            val.r#type.to_string().into(),
            val.value.into(),
            val.additional_options.map_or(Value::Null, |m| {
                serde_json::to_value(m).unwrap_or(Value::Null)
            }),
        ])
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub enum MessageContentType {
    #[default]
    Text,
    ImageUrl,
    InputAudio,
    File,
}

impl Display for MessageContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageContentType::Text => f.write_str("Text"),
            MessageContentType::ImageUrl => f.write_str("ImageUrl"),
            MessageContentType::InputAudio => f.write_str("InputAudio"),
            MessageContentType::File => f.write_str("File"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum MessageContentValue {
    Text(String),
    ImageUrl(Vec<MessageContentPart>),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum MessageContentPartOptions {
    Image(ImageDetail),
    Audio(AudioDetail),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AudioDetail {
    pub r#type: AudioFormat,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ImageDetail {
    Auto,
    Low,
    High,
}
