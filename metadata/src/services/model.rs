use crate::error::DatabaseError;
use crate::models::model::DbModel;
use crate::pool::DbPool;
use diesel::ExpressionMethods;
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
}
