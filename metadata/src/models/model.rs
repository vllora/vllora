use crate::schema::models;
use diesel::helper_types::AsSelect;
use diesel::helper_types::Select;
#[cfg(feature = "sqlite")]
use diesel::sqlite::Sqlite;
#[cfg(feature = "postgres")]
use diesel::pg::Pg;
use diesel::{BoolExpressionMethods, ExpressionMethods};
use diesel::QueryDsl;
use diesel::SelectableHelper;
use diesel::{Identifiable, Queryable};
use diesel::{AsChangeset, Insertable, QueryableByName, Selectable};
use serde::{Deserialize, Serialize};
use langdb_core::models::{ModelMetadata, ModelType, ModelCapability, ModelIOFormats, Limits, InferenceProvider};
use langdb_core::types::provider::{ModelPrice, CompletionModelPrice, InferenceModelProvider};
use chrono::NaiveDate;
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
    pub parameters: Option<String>,    // JSON object stored as text
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

impl Into<ModelMetadata> for DbModel {
    fn into(self) -> ModelMetadata {
        // Parse JSON arrays/objects
        let capabilities: Vec<ModelCapability> = self.capabilities
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let input_formats: Vec<ModelIOFormats> = self.input_types
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| vec![ModelIOFormats::Text]);

        let output_formats: Vec<ModelIOFormats> = self.output_types
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| vec![ModelIOFormats::Text]);

        let parameters: Option<serde_json::Value> = self.parameters
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        let benchmark_info: Option<serde_json::Value> = self.benchmark_info
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        // Parse dates
        let release_date = self.release_date
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let langdb_release_date = self.langdb_release_date
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let knowledge_cutoff_date = self.knowledge_cutoff_date
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        // Parse model type
        let model_type = ModelType::from_str(&self.model_type).unwrap_or(ModelType::Completions);

        // Determine inference provider from provider_info_id
        // For now, we'll use the owner_name as provider
        let inference_provider = InferenceProvider {
            provider: InferenceModelProvider::from(self.owner_name.clone()),
            model_name: self.model_name_in_provider.clone().unwrap_or_else(|| self.model_name.clone()),
            endpoint: None,
        };

        // Build price
        let price = ModelPrice::Completion(CompletionModelPrice {
            per_input_token: self.input_token_price.unwrap_or(0.0) as f64,
            per_output_token: self.output_token_price.unwrap_or(0.0) as f64,
            per_cached_input_token: self.cached_input_token_price.map(|p| p as f64),
            per_cached_input_write_token: self.cached_input_write_token_price.map(|p| p as f64),
            valid_from: None,
        });

        ModelMetadata {
            model: self.model_name.clone(),
            model_provider: self.owner_name.clone(),
            inference_provider,
            price,
            input_formats,
            output_formats,
            capabilities,
            r#type: model_type,
            limits: Limits::new(self.context_size.unwrap_or(0) as u32),
            description: self.description.unwrap_or_default(),
            parameters,
            benchmark_info,
            virtual_model_id: self.id.clone(),
            min_service_level: 0,
            release_date,
            license: self.license.clone(),
            knowledge_cutoff_date,
            langdb_release_date,
            is_private: self.project_id.is_some(),
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
        all.filter(models::deleted_at.is_null())
            .filter(
                models::project_id.eq(project_id)
                    .or(models::project_id.is_null())
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