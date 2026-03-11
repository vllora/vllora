use crate::metadata::error::DatabaseError;
use crate::metadata::models::eval_job::{DbEvalJob, DbNewEvalJob, DbUpdateEvalJob};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::eval_jobs::dsl;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct EvalJobService {
    db_pool: DbPool,
}

impl EvalJobService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create(
        &self,
        workflow_id: &str,
        cloud_run_id: Option<&str>,
        sample_size: Option<i32>,
        rollout_model: Option<&str>,
    ) -> Result<DbEvalJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let input = DbNewEvalJob::new(
            workflow_id.to_string(),
            cloud_run_id.map(|s| s.to_string()),
            sample_size,
            rollout_model.map(|s| s.to_string()),
        );
        let job_id = input.id.clone();

        diesel::insert_into(dsl::eval_jobs)
            .values(&input)
            .execute(&mut conn)?;

        Ok(dsl::eval_jobs
            .filter(dsl::id.eq(job_id))
            .first::<DbEvalJob>(&mut conn)?)
    }

    pub fn get(&self, job_id: &str) -> Result<DbEvalJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::eval_jobs
            .filter(dsl::id.eq(job_id))
            .first::<DbEvalJob>(&mut conn)?)
    }

    pub fn list_by_workflow(&self, workflow_id: &str) -> Result<Vec<DbEvalJob>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::eval_jobs
            .filter(dsl::workflow_id.eq(workflow_id))
            .order(dsl::created_at.desc())
            .load::<DbEvalJob>(&mut conn)?)
    }

    pub fn list_by_status(&self, status: &str) -> Result<Vec<DbEvalJob>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::eval_jobs
            .filter(dsl::status.eq(status))
            .order(dsl::created_at.desc())
            .load::<DbEvalJob>(&mut conn)?)
    }

    pub fn update_status(
        &self,
        job_id: &str,
        status: &str,
    ) -> Result<DbEvalJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let affected = diesel::update(dsl::eval_jobs)
            .filter(dsl::id.eq(job_id))
            .set(&DbUpdateEvalJob::with_status(status.to_string()))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }

        Ok(dsl::eval_jobs
            .filter(dsl::id.eq(job_id))
            .first::<DbEvalJob>(&mut conn)?)
    }

    pub fn update_error(
        &self,
        job_id: &str,
        status: &str,
        error: &str,
    ) -> Result<DbEvalJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let affected = diesel::update(dsl::eval_jobs)
            .filter(dsl::id.eq(job_id))
            .set(&DbUpdateEvalJob::with_error(
                status.to_string(),
                error.to_string(),
            ))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }

        Ok(dsl::eval_jobs
            .filter(dsl::id.eq(job_id))
            .first::<DbEvalJob>(&mut conn)?)
    }

    pub fn update_full(
        &self,
        job_id: &str,
        changeset: DbUpdateEvalJob,
    ) -> Result<DbEvalJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let affected = diesel::update(dsl::eval_jobs)
            .filter(dsl::id.eq(job_id))
            .set(&changeset)
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }

        Ok(dsl::eval_jobs
            .filter(dsl::id.eq(job_id))
            .first::<DbEvalJob>(&mut conn)?)
    }

    pub fn delete(&self, job_id: &str) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::delete(dsl::eval_jobs)
            .filter(dsl::id.eq(job_id))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(())
    }

    pub fn delete_by_workflow(&self, workflow_id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::delete(dsl::eval_jobs)
            .filter(dsl::workflow_id.eq(workflow_id))
            .execute(&mut conn)?;
        Ok(affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_utils::setup_test_database;

    #[test]
    fn test_create_and_get() {
        let db_pool = setup_test_database();
        let service = EvalJobService::new(db_pool.clone());

        let job = service
            .create("wf1", Some("cloud-run-123"), Some(50), Some("gpt-4o"))
            .unwrap();
        assert_eq!(job.status, "pending");
        assert_eq!(job.cloud_run_id, Some("cloud-run-123".to_string()));
        assert_eq!(job.sample_size, Some(50));
        assert_eq!(job.rollout_model, Some("gpt-4o".to_string()));

        let fetched = service.get(&job.id).unwrap();
        assert_eq!(fetched.id, job.id);
    }

    #[test]
    fn test_update_status() {
        let db_pool = setup_test_database();
        let service = EvalJobService::new(db_pool.clone());

        let job = service.create("wf1", Some("run1"), None, None).unwrap();
        service.update_status(&job.id, "running").unwrap();
        assert_eq!(service.get(&job.id).unwrap().status, "running");

        service.update_status(&job.id, "completed").unwrap();
        assert_eq!(service.get(&job.id).unwrap().status, "completed");
    }

    #[test]
    fn test_update_error() {
        let db_pool = setup_test_database();
        let service = EvalJobService::new(db_pool.clone());

        let job = service.create("wf1", Some("run1"), None, None).unwrap();
        let failed = service
            .update_error(&job.id, "failed", "timeout after 60s")
            .unwrap();
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error, Some("timeout after 60s".to_string()));
    }

    #[test]
    fn test_list_by_status_cross_workflow() {
        let db_pool = setup_test_database();
        let service = EvalJobService::new(db_pool.clone());

        service.create("wf1", Some("run1"), None, None).unwrap();
        service.create("wf2", Some("run2"), None, None).unwrap();
        let job3 = service.create("wf1", Some("run3"), None, None).unwrap();
        service.update_status(&job3.id, "completed").unwrap();

        let pending = service.list_by_status("pending").unwrap();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_delete_by_workflow() {
        let db_pool = setup_test_database();
        let service = EvalJobService::new(db_pool.clone());

        service.create("wf1", Some("run1"), None, None).unwrap();
        service.create("wf1", Some("run2"), None, None).unwrap();
        service.create("wf2", Some("run3"), None, None).unwrap();

        service.delete_by_workflow("wf1").unwrap();

        assert_eq!(service.list_by_workflow("wf1").unwrap().len(), 0);
        assert_eq!(service.list_by_workflow("wf2").unwrap().len(), 1);
    }
}
