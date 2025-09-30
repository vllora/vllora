use crate::error::DatabaseError;
use crate::models::model::{DbModel, DbNewModel};
use crate::pool::DbPool;
use chrono::Utc;
use diesel::{ExpressionMethods, OptionalExtension};
use diesel::{QueryDsl, RunQueryDsl};
use uuid::Uuid;

pub trait ModelService {
    fn list(&self, project_id: Option<Uuid>) -> Result<Vec<DbModel>, DatabaseError>;
    fn get_by_id(&self, id: String) -> Result<DbModel, DatabaseError>;
    fn get_by_name(
        &self,
        model_name: &str,
        project_id: Option<Uuid>,
    ) -> Result<DbModel, DatabaseError>;
    fn insert_many(&self, models: Vec<DbNewModel>) -> Result<(), DatabaseError>;
    fn upsert(&self, model: DbNewModel) -> Result<(), DatabaseError>;
    fn mark_as_deleted(&self, model_id: String) -> Result<(), DatabaseError>;
    fn mark_models_as_deleted(&self, model_ids: Vec<String>) -> Result<(), DatabaseError>;
}

pub struct ModelServiceImpl {
    db_pool: DbPool,
}

impl ModelServiceImpl {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }
}

impl ModelService for ModelServiceImpl {
    fn list(&self, project_id: Option<Uuid>) -> Result<Vec<DbModel>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let models = match project_id {
            Some(pid) => {
                let project_id_str = pid.to_string();
                DbModel::for_project(project_id_str).load::<DbModel>(&mut conn)?
            }
            None => DbModel::global_only().load::<DbModel>(&mut conn)?,
        };

        Ok(models)
    }

    fn get_by_id(&self, id: String) -> Result<DbModel, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        DbModel::not_deleted()
            .filter(crate::schema::models::id.eq(id))
            .first(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    fn get_by_name(
        &self,
        model_name: &str,
        project_id: Option<Uuid>,
    ) -> Result<DbModel, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let model = match project_id {
            Some(pid) => {
                let project_id_str = pid.to_string();
                DbModel::for_project(project_id_str)
                    .filter(crate::schema::models::model_name.eq(model_name))
                    .first(&mut conn)?
            }
            None => DbModel::global_only()
                .filter(crate::schema::models::model_name.eq(model_name))
                .first(&mut conn)?,
        };

        Ok(model)
    }

    fn insert_many(&self, models: Vec<DbNewModel>) -> Result<(), DatabaseError> {
        for model in models {
            self.upsert(model)?;
        }

        Ok(())
    }

    fn upsert(&self, model: DbNewModel) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Try to find existing model by model_name and provider_info_id
        let existing = DbModel::not_deleted()
            .filter(crate::schema::models::model_name.eq(&model.model_name))
            .filter(crate::schema::models::provider_info_id.eq(&model.provider_info_id))
            .first::<DbModel>(&mut conn)
            .optional()?;

        if let Some(existing_model) = existing {
            // Update existing model
            diesel::update(crate::schema::models::table)
                .filter(crate::schema::models::id.eq(existing_model.id))
                .set(&model)
                .execute(&mut conn)?;
        } else {
            // Insert new model
            diesel::insert_into(crate::schema::models::table)
                .values(&model)
                .execute(&mut conn)?;
        }

        Ok(())
    }

    fn mark_as_deleted(&self, model_id: String) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        diesel::update(crate::schema::models::table)
            .filter(crate::schema::models::id.eq(model_id))
            .set(crate::schema::models::deleted_at.eq(now))
            .execute(&mut conn)?;

        Ok(())
    }

    fn mark_models_as_deleted(&self, model_ids: Vec<String>) -> Result<(), DatabaseError> {
        if model_ids.is_empty() {
            return Ok(());
        }

        let mut conn = self.db_pool.get()?;
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        diesel::update(crate::schema::models::table)
            .filter(crate::schema::models::id.eq_any(model_ids))
            .set(crate::schema::models::deleted_at.eq(now))
            .execute(&mut conn)?;

        Ok(())
    }
}
