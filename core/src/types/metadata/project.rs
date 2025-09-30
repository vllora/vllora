use chrono::NaiveDateTime;
use serde_json::Value;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::types::provider::ModelPrice;

pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub company_id: Uuid,
    pub slug: String,
    pub settings: Option<Value>,
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