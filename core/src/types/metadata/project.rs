use chrono::NaiveDateTime;
use serde_json::Value;
use uuid::Uuid;


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