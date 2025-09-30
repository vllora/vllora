use crate::schema::models;
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
use langdb_core::models::{
    InferenceProvider, Limits, ModelCapability, ModelIOFormats, ModelMetadata, ModelType,
};
use langdb_core::types::provider::{CompletionModelPrice, InferenceModelProvider, ModelPrice};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

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
    pub provider_info_id: String,
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
}

impl From<DbModel> for ModelMetadata {
    fn from(val: DbModel) -> Self {
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

        // Determine inference provider from provider_info_id
        // For now, we'll use the owner_name as provider
        let inference_provider = InferenceProvider {
            provider: InferenceModelProvider::from(val.owner_name.clone()),
            model_name: val
                .model_name_in_provider
                .clone()
                .unwrap_or_else(|| val.model_name.clone()),
            endpoint: None,
        };

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
            description: val.description.unwrap_or_default(),
            parameters,
            benchmark_info,
            virtual_model_id: val.id.clone(),
            min_service_level: 0,
            release_date,
            license: val.license.clone(),
            knowledge_cutoff_date,
            langdb_release_date,
            is_private: val.project_id.is_some(),
        }
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
}

#[derive(Insertable, AsChangeset, PartialEq, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
#[diesel(table_name = models)]
pub struct DbNewModel {
    pub id: Option<String>,
    pub model_name: String,
    pub description: Option<String>,
    pub provider_info_id: String,
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
}
