use diesel::prelude::*;
use diesel::sql_types::{BigInt, Double, Nullable, Text};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, QueryableByName)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct RunUsageInformation {
    #[diesel(sql_type = Nullable<Text>)]
    pub run_id: Option<String>,

    #[diesel(sql_type = Text)]
    thread_ids_json: String,

    #[diesel(sql_type = Text)]
    trace_ids_json: String,

    #[diesel(sql_type = Text)]
    request_models_json: String,

    #[diesel(sql_type = Text)]
    used_models_json: String,

    #[diesel(sql_type = Text)]
    used_tools_json: String,

    #[diesel(sql_type = Text)]
    mcp_template_definition_ids_json: String,

    #[diesel(sql_type = BigInt)]
    pub llm_calls: i64,

    #[diesel(sql_type = Double)]
    pub cost: f64,

    #[diesel(sql_type = BigInt)]
    pub input_tokens: i64,

    #[diesel(sql_type = BigInt)]
    pub output_tokens: i64,

    #[diesel(sql_type = BigInt)]
    pub start_time_us: i64,

    #[diesel(sql_type = BigInt)]
    pub finish_time_us: i64,

    #[diesel(sql_type = Text)]
    errors_json: String,
}

impl RunUsageInformation {
    pub fn thread_ids(&self) -> Vec<String> {
        serde_json::from_str(&self.thread_ids_json).unwrap_or_default()
    }

    pub fn trace_ids(&self) -> Vec<String> {
        serde_json::from_str(&self.trace_ids_json).unwrap_or_default()
    }

    pub fn request_models(&self) -> Vec<String> {
        serde_json::from_str(&self.request_models_json).unwrap_or_default()
    }

    pub fn used_models(&self) -> Vec<String> {
        serde_json::from_str(&self.used_models_json).unwrap_or_default()
    }

    pub fn used_tools(&self) -> Vec<String> {
        serde_json::from_str(&self.used_tools_json).unwrap_or_default()
    }

    pub fn mcp_template_definition_ids(&self) -> Vec<String> {
        serde_json::from_str(&self.mcp_template_definition_ids_json).unwrap_or_default()
    }

    pub fn errors(&self) -> Vec<String> {
        serde_json::from_str(&self.errors_json).unwrap_or_default()
    }
}

// For serialization responses - with parsed vectors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunUsageResponse {
    pub run_id: Option<String>,
    pub thread_ids: Vec<String>,
    pub trace_ids: Vec<String>,
    pub request_models: Vec<String>,
    pub used_models: Vec<String>,
    pub used_tools: Vec<String>,
    pub mcp_template_definition_ids: Vec<String>,
    pub llm_calls: i64,
    pub cost: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub start_time_us: i64,
    pub finish_time_us: i64,
    pub errors: Vec<String>,
}

impl From<RunUsageInformation> for RunUsageResponse {
    fn from(info: RunUsageInformation) -> Self {
        Self {
            run_id: info.run_id.clone(),
            thread_ids: info.thread_ids(),
            trace_ids: info.trace_ids(),
            request_models: info.request_models(),
            used_models: info.used_models(),
            used_tools: info.used_tools(),
            mcp_template_definition_ids: info.mcp_template_definition_ids(),
            llm_calls: info.llm_calls,
            cost: info.cost,
            input_tokens: info.input_tokens,
            output_tokens: info.output_tokens,
            start_time_us: info.start_time_us,
            finish_time_us: info.finish_time_us,
            errors: info.errors(),
        }
    }
}
