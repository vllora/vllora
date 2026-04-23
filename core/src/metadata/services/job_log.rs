use crate::metadata::error::DatabaseError;
use crate::metadata::models::job_log::{DbJobLog, DbNewJobLog};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::jobs_logs::dsl;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct JobLogService {
    db_pool: DbPool,
}

impl JobLogService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create(&self, input: DbNewJobLog) -> Result<DbJobLog, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let id = input.id.clone();
        diesel::insert_into(dsl::jobs_logs)
            .values(&input)
            .execute(&mut conn)?;
        Ok(dsl::jobs_logs
            .filter(dsl::id.eq(id))
            .first::<DbJobLog>(&mut conn)?)
    }

    pub fn list_by_job(&self, job_id: &str) -> Result<Vec<DbJobLog>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::jobs_logs
            .filter(dsl::job_id.eq(job_id))
            .order(dsl::created_at.asc())
            .load::<DbJobLog>(&mut conn)?)
    }
}
