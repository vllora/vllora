use crate::metadata::schema::models;
use chrono::NaiveDate;
use diesel::helper_types::AsSelect;
use diesel::helper_types::Select;
#[cfg(feature = "postgres")]
use diesel::pg::Pg;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
use diesel::QueryDsl;
use diesel::SelectableHelper;
use diesel::{AsChangeset, Insertable, QueryableByName, Selectable};
use diesel::{BoolExpressionMethods, ExpressionMethods};
use diesel::{Identifiable, Queryable};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use vllora_llm::types::engine::CustomInferenceApiType;
use vllora_llm::types::models::InferenceProvider;
use vllora_llm::types::models::Limits;
use vllora_llm::types::models::ModelCapability;
use vllora_llm::types::models::ModelIOFormats;
use vllora_llm::types::models::ModelMetadata;
use vllora_llm::types::models::ModelType;
use vllora_llm::types::provider::{CompletionModelPrice, InferenceModelProvider, ModelPrice};

#[derive(
    QueryableByName,
    Selectable,
    Queryable,
    PartialEq,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Identifiable,
    AsChangeset,
)]
#[serde(crate = "serde")]
#[diesel(table_name = models)]
pub struct DbModel {
    pub id: Option<String>,
    pub model_name: String,
    pub description: Option<String>,
    pub provider_name: String,
    pub model_type: String,
    pub input_token_price: Option<f32>,
    pub output_token_price: Option<f32>,
    pub context_size: Option<i32>,
    pub capabilities: Option<String>, // JSON array stored as text
    pub input_types: Option<String>,  // JSON array stored as text
    pub output_types: Option<String>, // JSON array stored as text
    pub tags: Option<String>,         // JSON array stored as text
    pub type_prices: Option<String>,  // JSON object stored as text
    pub mp_price: Option<f32>,
    pub model_name_in_provider: Option<String>,
    pub owner_name: String,
    pub priority: i32,
    pub parameters: Option<String>, // JSON object stored as text
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub benchmark_info: Option<String>, // JSON object stored as text
    pub cached_input_token_price: Option<f32>,
    pub cached_input_write_token_price: Option<f32>,
    pub release_date: Option<String>,
    pub langdb_release_date: Option<String>,
    pub knowledge_cutoff_date: Option<String>,
    pub license: Option<String>,
    pub project_id: Option<String>,
    pub endpoint: Option<String>,
    pub is_custom: i32,
}

impl From<DbModel> for ModelMetadata {
    fn from(val: DbModel) -> Self {
        // Use the helper function with None for custom_inference_api_type
        // This maintains backward compatibility - callers should use to_model_metadata_with_provider
        // when provider info is available
        DbModel::to_model_metadata_with_provider(&val, None)
    }
}

#[cfg(feature = "sqlite")]
type All = Select<models::table, AsSelect<DbModel, Sqlite>>;
#[cfg(feature = "postgres")]
type All = Select<models::table, AsSelect<DbModel, Pg>>;

impl DbModel {
    pub fn all() -> All {
        diesel::QueryDsl::select(models::table, DbModel::as_select())
    }

    #[diesel::dsl::auto_type(no_type_alias)]
    pub fn not_deleted() -> _ {
        let all: All = DbModel::all();
        all.filter(models::deleted_at.is_null())
    }

    // Query models for a specific project (includes global models with null project_id)
    #[diesel::dsl::auto_type(no_type_alias)]
    pub fn for_project(project_id: String) -> _ {
        let all: All = DbModel::all();
        all.filter(models::deleted_at.is_null()).filter(
            models::project_id
                .eq(project_id)
                .or(models::project_id.is_null()),
        )
    }

    // Query only global models (project_id is null)
    #[diesel::dsl::auto_type(no_type_alias)]
    pub fn global_only() -> _ {
        let all: All = DbModel::all();
        all.filter(models::deleted_at.is_null())
            .filter(models::project_id.is_null())
    }

    /// Build InferenceProvider with optional custom_inference_api_type
    /// If custom_inference_api_type is provided, it determines the InferenceModelProvider variant
    pub fn build_inference_provider(
        provider_name: &str,
        model_name: &str,
        endpoint: Option<&str>,
        custom_inference_api_type: Option<CustomInferenceApiType>,
    ) -> InferenceProvider {
        let provider = InferenceModelProvider::from(provider_name.to_string());

        InferenceProvider {
            provider,
            model_name: model_name.to_string(),
            endpoint: endpoint.map(|s| s.to_string()),
            custom_inference_api_type,
        }
    }

    /// Convert DbModel to ModelMetadata with optional provider custom_inference_api_type
    pub fn to_model_metadata_with_provider(
        val: &DbModel,
        custom_inference_api_type: Option<CustomInferenceApiType>,
    ) -> ModelMetadata {
        // Parse JSON arrays/objects
        let capabilities: Vec<ModelCapability> = val
            .capabilities
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let input_formats: Vec<ModelIOFormats> = val
            .input_types
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| vec![ModelIOFormats::Text]);

        let output_formats: Vec<ModelIOFormats> = val
            .output_types
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| vec![ModelIOFormats::Text]);

        let parameters: Option<serde_json::Value> = val
            .parameters
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        let benchmark_info: Option<serde_json::Value> = val
            .benchmark_info
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        // Parse dates
        let release_date = val
            .release_date
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let langdb_release_date = val
            .langdb_release_date
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let knowledge_cutoff_date = val
            .knowledge_cutoff_date
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        // Parse model type
        let model_type = ModelType::from_str(&val.model_type).unwrap_or(ModelType::Completions);

        // Build inference provider with custom_inference_api_type
        let inference_provider = Self::build_inference_provider(
            &val.provider_name,
            val.model_name_in_provider
                .as_deref()
                .unwrap_or(&val.model_name),
            val.endpoint.as_deref(),
            custom_inference_api_type,
        );

        // Build price
        let price = ModelPrice::Completion(CompletionModelPrice {
            per_input_token: val.input_token_price.unwrap_or(0.0) as f64,
            per_output_token: val.output_token_price.unwrap_or(0.0) as f64,
            per_cached_input_token: val.cached_input_token_price.map(|p| p as f64),
            per_cached_input_write_token: val.cached_input_write_token_price.map(|p| p as f64),
            valid_from: None,
        });

        ModelMetadata {
            model: val.model_name.clone(),
            model_provider: val.owner_name.clone(),
            inference_provider,
            price,
            input_formats,
            output_formats,
            capabilities,
            r#type: model_type,
            limits: Limits::new(val.context_size.unwrap_or(0) as u32),
            description: val.description.clone().unwrap_or_default(),
            parameters,
            benchmark_info,
            virtual_model_id: val.id.clone(),
            min_service_level: 0,
            release_date,
            license: val.license.clone(),
            knowledge_cutoff_date,
            langdb_release_date,
            is_private: val.project_id.is_some(),
            is_custom: val.is_custom != 0,
        }
    }
}

#[derive(Insertable, AsChangeset, PartialEq, Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "serde")]
#[diesel(table_name = models)]
pub struct DbNewModel {
    pub id: Option<String>,
    pub model_name: String,
    pub description: Option<String>,
    pub provider_name: String,
    pub model_type: String,
    pub input_token_price: Option<f32>,
    pub output_token_price: Option<f32>,
    pub context_size: Option<i32>,
    pub capabilities: Option<String>,
    pub input_types: Option<String>,
    pub output_types: Option<String>,
    pub tags: Option<String>,
    pub type_prices: Option<String>,
    pub mp_price: Option<f32>,
    pub model_name_in_provider: Option<String>,
    pub owner_name: String,
    pub priority: i32,
    pub parameters: Option<String>,
    pub benchmark_info: Option<String>,
    pub cached_input_token_price: Option<f32>,
    pub cached_input_write_token_price: Option<f32>,
    pub release_date: Option<String>,
    pub langdb_release_date: Option<String>,
    pub knowledge_cutoff_date: Option<String>,
    pub license: Option<String>,
    pub project_id: Option<String>,
    pub deleted_at: Option<String>,
    pub endpoint: Option<String>,
    pub is_custom: i32,
}
impl From<ModelMetadata> for DbNewModel {
    fn from(metadata: ModelMetadata) -> Self {
        // Serialize arrays/objects to JSON strings
        let capabilities = if !metadata.capabilities.is_empty() {
            Some(serde_json::to_string(&metadata.capabilities).unwrap_or_default())
        } else {
            None
        };

        let input_types = if !metadata.input_formats.is_empty() {
            Some(serde_json::to_string(&metadata.input_formats).unwrap_or_default())
        } else {
            None
        };

        let output_types = if !metadata.output_formats.is_empty() {
            Some(serde_json::to_string(&metadata.output_formats).unwrap_or_default())
        } else {
            None
        };

        let parameters = metadata
            .parameters
            .map(|p| serde_json::to_string(&p).unwrap_or_default());

        let benchmark_info = metadata
            .benchmark_info
            .map(|b| serde_json::to_string(&b).unwrap_or_default());

        // Extract pricing information
        let (
            input_token_price,
            output_token_price,
            cached_input_token_price,
            cached_input_write_token_price,
        ) = match metadata.price {
            ModelPrice::Completion(price) => (
                Some(price.per_input_token as f32),
                Some(price.per_output_token as f32),
                price.per_cached_input_token.map(|p| p as f32),
                price.per_cached_input_write_token.map(|p| p as f32),
            ),
            ModelPrice::Embedding(_) => (None, None, None, None),
            ModelPrice::ImageGeneration(_) => (None, None, None, None),
        };

        // Format dates
        let release_date = metadata
            .release_date
            .map(|d| d.format("%Y-%m-%d").to_string());

        let langdb_release_date = metadata
            .langdb_release_date
            .map(|d| d.format("%Y-%m-%d").to_string());

        let knowledge_cutoff_date = metadata
            .knowledge_cutoff_date
            .map(|d| d.format("%Y-%m-%d").to_string());

        DbNewModel {
            id: metadata.virtual_model_id,
            model_name: metadata.model.clone(),
            description: Some(metadata.description),
            provider_name: metadata.inference_provider.provider.to_string(),
            model_type: metadata.r#type.to_string(),
            input_token_price,
            output_token_price,
            context_size: Some(metadata.limits.max_context_size as i32),
            capabilities,
            input_types,
            output_types,
            tags: None,
            type_prices: None,
            mp_price: None,
            model_name_in_provider: Some(metadata.inference_provider.model_name),
            owner_name: metadata.model_provider,
            priority: metadata.min_service_level,
            parameters,
            benchmark_info,
            cached_input_token_price,
            cached_input_write_token_price,
            release_date,
            langdb_release_date,
            knowledge_cutoff_date,
            license: metadata.license,
            project_id: None, // API models are global
            deleted_at: None, // Clear deleted_at if model comes back from API
            endpoint: metadata.inference_provider.endpoint,
            is_custom: 0, // Default to false, should be set explicitly when creating via API
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vllora_llm::types::engine::CustomInferenceApiType;

    #[test]
    fn test_build_inference_provider_with_custom_api_type() {
        let provider = DbModel::build_inference_provider(
            "custom-provider",
            "model-name",
            Some("https://api.example.com"),
            Some(CustomInferenceApiType::Gemini),
        );

        assert_eq!(
            provider.provider,
            InferenceModelProvider::Proxy("custom-provider".to_string())
        );
        assert_eq!(provider.model_name, "model-name");
        assert_eq!(
            provider.custom_inference_api_type,
            Some(CustomInferenceApiType::Gemini)
        );
    }

    #[test]
    fn test_build_inference_provider_without_custom_api_type() {
        let provider = DbModel::build_inference_provider("openai", "gpt-4", None, None);

        assert_eq!(provider.provider, InferenceModelProvider::OpenAI);
        assert_eq!(provider.custom_inference_api_type, None);
    }

    #[test]
    fn test_to_model_metadata_with_provider() {
        let db_model = DbModel {
            id: Some("test-id".to_string()),
            model_name: "test-model".to_string(),
            description: Some("Test model".to_string()),
            provider_name: "custom-provider".to_string(),
            model_type: "completions".to_string(),
            input_token_price: Some(0.001),
            output_token_price: Some(0.002),
            context_size: Some(4096),
            capabilities: None,
            input_types: None,
            output_types: None,
            tags: None,
            type_prices: None,
            mp_price: None,
            model_name_in_provider: None,
            owner_name: "custom-provider".to_string(),
            priority: 0,
            parameters: None,
            created_at: "2023-01-01T00:00:00Z".to_string(),
            updated_at: "2023-01-01T00:00:00Z".to_string(),
            deleted_at: None,
            benchmark_info: None,
            cached_input_token_price: None,
            cached_input_write_token_price: None,
            release_date: None,
            langdb_release_date: None,
            knowledge_cutoff_date: None,
            license: None,
            project_id: None,
            endpoint: None,
            is_custom: 0,
        };

        let metadata = DbModel::to_model_metadata_with_provider(
            &db_model,
            Some(CustomInferenceApiType::Anthropic),
        );

        assert_eq!(metadata.model, "test-model");
        assert_eq!(
            metadata.inference_provider.provider,
            InferenceModelProvider::Proxy("custom-provider".to_string())
        );
        assert_eq!(
            metadata.inference_provider.custom_inference_api_type,
            Some(CustomInferenceApiType::Anthropic)
        );
    }
}
