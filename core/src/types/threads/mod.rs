pub mod public_threads;
pub mod related_threads;
pub mod service;

use std::fmt::Display;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_tuple::Serialize_tuple;
use serde_with::serde_as;

use super::{gateway::ToolCall, message::MessageType};
use crate::types::gateway::CacheControl;

use crate::types::threads::{
    public_threads::PublicThreads, related_threads::RelatedThreads, service::ThreadService,
};
pub trait ThreadServiceWrapper: 'static {
    fn related_threads(&self) -> Arc<dyn RelatedThreads>;
    fn public_threads(&self) -> Arc<dyn PublicThreads>;
    fn service(&self) -> Arc<dyn ThreadService>;
}

#[derive(Clone, Debug)]
pub struct CompletionsRunId(String);

impl CompletionsRunId {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn value(&self) -> String {
        self.0.clone()
    }
}

#[derive(Clone, Debug)]
pub struct CompletionsThreadId(String);

impl CompletionsThreadId {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn value(&self) -> String {
        self.0.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageThread {
    pub id: String,         // UUID
    pub model_name: String, // Corresponding LangDB model
    pub user_id: String,    // UUID
    pub project_id: String, // Project identifier
    pub is_public: bool,
    pub title: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PublicMessageThread {
    pub id: String, // UUID
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageThreadWithTitle {
    pub id: String, // UUID
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(alias = "used_models")]
    pub input_models: Vec<String>,
    pub mcp_template_definition_ids: Vec<String>,
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub description: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub is_public: bool,
    pub project_id: String,
    pub errors: Option<Vec<String>>,
    pub tags_info: Option<Vec<String>>,
    #[serde(alias = "model_name")]
    pub request_model_name: String,
}

impl From<MessageThread> for MessageThreadWithTitle {
    fn from(thread: MessageThread) -> Self {
        Self {
            id: thread.id,
            title: thread.title.unwrap_or_default(),
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            input_models: vec![],
            mcp_template_definition_ids: vec![],
            cost: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            description: thread.description,
            keywords: thread.keywords,
            is_public: thread.is_public,
            project_id: thread.project_id,
            errors: None,
            tags_info: None,
            request_model_name: thread.model_name,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageOptions {
    pub order_by: Vec<(String, PageOrderType)>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PageOrderType {
    Asc,
    Desc,
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
                cache_control: None,
            },
            MessageContentPart {
                r#type: super::MessageContentType::Text,
                value: "How is my image".to_string(),
                additional_options: None,
                cache_control: None,
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

    #[test]
    fn message_deserialization_3_tuple() {
        // Test deserialization of 3-tuple format (without cache_control)
        let json_data = json!([["Text", "Hello world", null]]);

        let result: Vec<MessageContentPart> = serde_json::from_value(json_data).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].r#type, super::MessageContentType::Text);
        assert_eq!(result[0].value, "Hello world");
        assert_eq!(result[0].additional_options, None);
        assert_eq!(result[0].cache_control, None);
    }

    #[test]
    fn message_deserialization_4_tuple() {
        // Test deserialization of 4-tuple format (with cache_control)
        let json_data = json!([["Text", "Hello world", null, null]]);

        let result: Vec<MessageContentPart> = serde_json::from_value(json_data).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].r#type, super::MessageContentType::Text);
        assert_eq!(result[0].value, "Hello world");
        assert_eq!(result[0].additional_options, None);
        assert_eq!(result[0].cache_control, None);
    }

    #[test]
    fn message_deserialization_4_tuple_with_cache_control() {
        // Test deserialization of 4-tuple format with actual cache_control
        let json_data = json!([
            ["Text", "Hello world", null, {"type": "ephemeral", "ttl": "5m"}]
        ]);

        let result: Vec<MessageContentPart> = serde_json::from_value(json_data).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].r#type, super::MessageContentType::Text);
        assert_eq!(result[0].value, "Hello world");
        assert_eq!(result[0].additional_options, None);
        assert!(result[0].cache_control.is_some());
    }
}
