use crate::metadata::error::DatabaseError;
use crate::metadata::models::workflow_log::{DbNewWorkflowLog, DbWorkflowLog};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::workflow_logs::dsl;
use diesel::dsl::count_star;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct WorkflowLogService {
    db_pool: DbPool,
}

impl WorkflowLogService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create_bulk(
        &self,
        workflow_id: &str,
        logs: Vec<DbNewWorkflowLog>,
    ) -> Result<Vec<DbWorkflowLog>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        if logs.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<String> = logs.iter().map(|l| l.id.clone()).collect();
        diesel::insert_into(dsl::workflow_logs)
            .values(&logs)
            .execute(&mut conn)?;

        Ok(dsl::workflow_logs
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::id.eq_any(ids))
            .order(dsl::created_at.asc())
            .load::<DbWorkflowLog>(&mut conn)?)
    }

    pub fn list_by_workflow(
        &self,
        workflow_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DbWorkflowLog>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::workflow_logs
            .filter(dsl::workflow_id.eq(workflow_id))
            .order((dsl::created_at.desc(), dsl::id.desc()))
            .limit(limit)
            .offset(offset)
            .load::<DbWorkflowLog>(&mut conn)?)
    }

    pub fn count_by_workflow(&self, workflow_id: &str) -> Result<i64, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::workflow_logs
            .filter(dsl::workflow_id.eq(workflow_id))
            .select(count_star())
            .first::<i64>(&mut conn)?)
    }
}
