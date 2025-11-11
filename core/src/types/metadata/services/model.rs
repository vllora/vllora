use crate::metadata::error::DatabaseError;
use crate::metadata::models::model::{DbModel, DbNewModel};
use uuid::Uuid;

pub trait ModelService: Send + Sync {
    fn list(&self, project_id: Option<Uuid>) -> Result<Vec<DbModel>, DatabaseError>;
    fn get_by_id(&self, id: String) -> Result<DbModel, DatabaseError>;
    fn get_by_name(
        &self,
        model_name: &str,
        project_id: Option<Uuid>,
    ) -> Result<Vec<DbModel>, DatabaseError>;
    fn get_by_provider_and_name(
        &self,
        model_name: &str,
        provider_name: &str,
        project_id: Option<Uuid>,
    ) -> Result<Option<DbModel>, DatabaseError>;
    fn insert_many(&self, models: Vec<DbNewModel>) -> Result<(), DatabaseError>;
    fn upsert(&self, model: DbNewModel) -> Result<(), DatabaseError>;
    fn mark_as_deleted(&self, model_id: String) -> Result<(), DatabaseError>;
    fn mark_models_as_deleted(&self, model_ids: Vec<String>) -> Result<(), DatabaseError>;
}
