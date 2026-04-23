use crate::metadata::error::DatabaseError;
use crate::metadata::models::job::{DbJob, DbNewJob, DbUpdateJob};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::jobs::dsl;
use diesel::BoolExpressionMethods;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct JobService {
    db_pool: DbPool,
}

impl JobService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create(&self, input: DbNewJob) -> Result<DbJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let id = input.id.clone();
        diesel::insert_into(dsl::jobs)
            .values(&input)
            .execute(&mut conn)?;
        Ok(dsl::jobs.filter(dsl::id.eq(id)).first::<DbJob>(&mut conn)?)
    }

    pub fn get_by_id(&self, job_id: &str) -> Result<Option<DbJob>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::jobs
            .filter(dsl::id.eq(job_id))
            .first::<DbJob>(&mut conn)
            .optional()?)
    }

    pub fn get_by_idempotency(
        &self,
        workflow_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<DbJob>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::jobs
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::idempotency_key.eq(idempotency_key))
            .order(dsl::created_at.desc())
            .first::<DbJob>(&mut conn)
            .optional()?)
    }

    pub fn update(&self, job_id: &str, changes: DbUpdateJob) -> Result<DbJob, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::update(dsl::jobs.filter(dsl::id.eq(job_id)))
            .set(&changes)
            .execute(&mut conn)?;
        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(dsl::jobs
            .filter(dsl::id.eq(job_id))
            .first::<DbJob>(&mut conn)?)
    }

    pub fn list_by_state(
        &self,
        state: &str,
        limit: i64,
    ) -> Result<Vec<DbJob>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::jobs
            .filter(dsl::state.eq(state))
            .order(dsl::created_at.asc())
            .limit(limit)
            .load::<DbJob>(&mut conn)?)
    }

    pub fn try_claim_queued(&self, job_id: &str) -> Result<bool, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let now = chrono::Utc::now().to_rfc3339();
        let affected = diesel::update(
            dsl::jobs.filter(dsl::id.eq(job_id).and(dsl::state.eq("queued"))),
        )
        .set((
            dsl::state.eq("running"),
            dsl::started_at.eq(Some(now.clone())),
            dsl::updated_at.eq(now),
        ))
        .execute(&mut conn)?;
        Ok(affected > 0)
    }
}
