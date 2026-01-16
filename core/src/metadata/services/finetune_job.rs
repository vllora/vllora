use crate::metadata::error::DatabaseError;
use crate::metadata::models::finetune_job::{
    DbFinetuneJob, DbNewFinetuneJob, DbUpdateFinetuneJob, FinetuneJobState,
};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::finetune_jobs::dsl;
use diesel::prelude::*;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct FinetuneJobService {
    db_pool: DbPool,
}

impl FinetuneJobService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    /// Create a new finetune job record
    pub fn create(&self, input: DbNewFinetuneJob) -> Result<DbFinetuneJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // SQLite doesn't support RETURNING clause, so we need to insert and then query
        diesel::insert_into(dsl::finetune_jobs)
            .values(&input)
            .execute(&mut conn)?;

        // Query the inserted record - use provider_job_id as unique identifier
        Ok(dsl::finetune_jobs
            .filter(dsl::provider_job_id.eq(&input.provider_job_id))
            .order(dsl::created_at.desc())
            .first::<DbFinetuneJob>(&mut conn)?)
    }

    /// Get finetune job by ID and project ID
    pub fn get_by_id(
        &self,
        job_id: &str,
        project_id: &str,
    ) -> Result<Option<DbFinetuneJob>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        Ok(dsl::finetune_jobs
            .filter(dsl::id.eq(job_id))
            .filter(dsl::project_id.eq(project_id))
            .get_result::<DbFinetuneJob>(&mut conn)
            .optional()?)
    }

    /// Get finetune job by provider job ID and project ID
    pub fn get_by_provider_job_id(
        &self,
        provider_job_id: &str,
        project_id: &str,
    ) -> Result<Option<DbFinetuneJob>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        Ok(dsl::finetune_jobs
            .filter(dsl::project_id.eq(project_id))
            .filter(dsl::provider_job_id.eq(provider_job_id))
            .get_result::<DbFinetuneJob>(&mut conn)
            .optional()?)
    }

    /// Update finetune job state and other fields
    pub fn update(
        &self,
        job_id: &str,
        project_id: &str,
        input: DbUpdateFinetuneJob,
    ) -> Result<DbFinetuneJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // SQLite doesn't support RETURNING clause
        diesel::update(dsl::finetune_jobs)
            .filter(dsl::id.eq(job_id))
            .filter(dsl::project_id.eq(project_id))
            .set(&input)
            .execute(&mut conn)?;

        // Query the updated record
        Ok(dsl::finetune_jobs
            .filter(dsl::id.eq(job_id))
            .filter(dsl::project_id.eq(project_id))
            .first::<DbFinetuneJob>(&mut conn)?)
    }

    /// Update finetune job state
    pub fn update_state(
        &self,
        job_id: &str,
        project_id: &str,
        state: FinetuneJobState,
    ) -> Result<DbFinetuneJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut update = DbUpdateFinetuneJob::new().with_state(state);

        // If state is terminal (succeeded, failed, cancelled), set completed_at
        if matches!(
            state,
            FinetuneJobState::Succeeded | FinetuneJobState::Failed | FinetuneJobState::Cancelled
        ) {
            update = update.with_completed_at(Some(chrono::Utc::now().to_rfc3339()));
        }

        // SQLite doesn't support RETURNING clause
        diesel::update(dsl::finetune_jobs)
            .filter(dsl::id.eq(job_id))
            .filter(dsl::project_id.eq(project_id))
            .set(&update)
            .execute(&mut conn)?;

        // Query the updated record
        Ok(dsl::finetune_jobs
            .filter(dsl::id.eq(job_id))
            .filter(dsl::project_id.eq(project_id))
            .first::<DbFinetuneJob>(&mut conn)?)
    }

    /// List finetune jobs by project ID with pagination
    pub fn list_by_project(
        &self,
        project_id: &str,
        limit: Option<u32>,
        after: Option<&str>,
        dataset_id: Option<&str>,
    ) -> Result<Vec<DbFinetuneJob>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut query = dsl::finetune_jobs
            .filter(dsl::project_id.eq(project_id))
            .order(dsl::created_at.desc())
            .into_boxed();

        if let Some(after_id) = after {
            query = query.filter(dsl::id.lt(after_id));
        }

        if let Some(limit_val) = limit {
            query = query.limit(limit_val as i64);
        } else {
            query = query.limit(100); // Default limit
        }

        if let Some(dataset_id) = dataset_id {
            query = query.filter(dsl::dataset_id.eq(dataset_id));
        }

        Ok(query.load::<DbFinetuneJob>(&mut conn)?)
    }

    /// List all jobs with pending or running state (for state tracker)
    pub fn list_pending_or_running(&self) -> Result<Vec<DbFinetuneJob>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        Ok(dsl::finetune_jobs
            .filter(dsl::state.eq("pending").or(dsl::state.eq("running")))
            .load::<DbFinetuneJob>(&mut conn)?)
    }
}
