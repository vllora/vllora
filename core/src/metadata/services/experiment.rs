use crate::metadata::error::DatabaseError;
use crate::metadata::models::experiment::{DbExperiment, NewDbExperiment, UpdateDbExperiment};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::experiments;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl};

pub struct ExperimentServiceImpl {
    db_pool: DbPool,
}

impl ExperimentServiceImpl {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create(&self, experiment: NewDbExperiment) -> Result<DbExperiment, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        diesel::insert_into(experiments::table)
            .values(&experiment)
            .execute(&mut conn)?;

        // Get the inserted experiment
        let inserted = DbExperiment::all()
            .filter(experiments::original_span_id.eq(&experiment.original_span_id))
            .order(experiments::created_at.desc())
            .first(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(inserted)
    }

    pub fn get_by_id(&self, id: &str) -> Result<DbExperiment, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        DbExperiment::all()
            .filter(experiments::id.eq(id))
            .first(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn get_by_span_id(&self, span_id: &str) -> Result<Vec<DbExperiment>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        DbExperiment::all()
            .filter(experiments::original_span_id.eq(span_id))
            .order(experiments::created_at.desc())
            .load(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn list(&self, project_id: Option<&str>) -> Result<Vec<DbExperiment>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut query = DbExperiment::all().into_boxed();

        if let Some(pid) = project_id {
            query = query.filter(experiments::project_id.eq(pid));
        }

        query
            .order(experiments::created_at.desc())
            .load(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn update(&self, id: &str, update_data: UpdateDbExperiment) -> Result<DbExperiment, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Check if the experiment exists
        let existing = DbExperiment::all()
            .filter(experiments::id.eq(id))
            .first::<DbExperiment>(&mut conn)
            .optional()
            .map_err(DatabaseError::QueryError)?;

        if existing.is_none() {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }

        diesel::update(experiments::table)
            .filter(experiments::id.eq(id))
            .set(&update_data)
            .execute(&mut conn)?;

        // Get the updated experiment
        DbExperiment::all()
            .filter(experiments::id.eq(id))
            .first(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn delete(&self, id: &str) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        diesel::delete(experiments::table)
            .filter(experiments::id.eq(id))
            .execute(&mut conn)?;

        Ok(())
    }
}
