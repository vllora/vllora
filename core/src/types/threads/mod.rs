pub mod public_threads;
pub mod related_threads;
pub mod service;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

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
#[serde(rename_all = "lowercase")]
pub enum PageOrderType {
    #[serde(alias = "Asc", alias = "ASC", alias = "asc")]
    Asc,
    #[serde(alias = "Desc", alias = "DESC", alias = "desc")]
    Desc,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vllora_llm::types::message::MessageContentPart;
    use vllora_llm::types::message::MessageContentType;

    #[test]
    fn message_serialization() {
        let test = vec![
            MessageContentPart {
                r#type: MessageContentType::ImageUrl,
                value: "image/base64".to_string(),
                additional_options: None,
                cache_control: None,
            },
            MessageContentPart {
                r#type: MessageContentType::Text,
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
        assert_eq!(result[0].r#type, MessageContentType::Text);
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
        assert_eq!(result[0].r#type, MessageContentType::Text);
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
        assert_eq!(result[0].r#type, MessageContentType::Text);
        assert_eq!(result[0].value, "Hello world");
        assert_eq!(result[0].additional_options, None);
        assert!(result[0].cache_control.is_some());
    }
}
