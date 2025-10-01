use crate::types::project_settings::ProjectSettings;
use crate::types::provider::ModelPrice;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub company_id: Uuid,
    pub slug: String,
    pub settings: Option<ProjectSettings>,
    pub is_default: bool,
    pub archived_at: Option<NaiveDateTime>,
    pub allowed_user_ids: Option<Vec<String>>,
    pub private_model_prices: Option<Value>,
}

#[derive(PartialEq, Debug, Serialize, Deserialize, Default, Clone)]
pub struct NewProjectDTO {
    pub name: String,
    pub description: Option<String>,
    pub settings: Option<Value>,
    pub private_model_prices: Option<HashMap<String, ModelPrice>>,
    pub usage_limit: Option<Value>,
}
