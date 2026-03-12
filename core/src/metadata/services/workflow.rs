use crate::metadata::error::DatabaseError;
use crate::metadata::models::workflow::{DbNewWorkflow, DbUpdateWorkflow, DbWorkflow};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::workflows::dsl;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct WorkflowService {
    db_pool: DbPool,
}

impl WorkflowService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create(&self, input: DbNewWorkflow) -> Result<DbWorkflow, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let workflow_id = input
            .id
            .clone()
            .ok_or_else(|| DatabaseError::InvalidArgument("workflow id is required".to_string()))?;

        diesel::insert_into(dsl::workflows)
            .values(&input)
            .execute(&mut conn)?;

        Ok(dsl::workflows
            .filter(dsl::id.eq(workflow_id))
            .first::<DbWorkflow>(&mut conn)?)
    }

    pub fn get_by_id(&self, workflow_id: &str) -> Result<DbWorkflow, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::workflows
            .filter(dsl::id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .first::<DbWorkflow>(&mut conn)?)
    }

    pub fn list(&self) -> Result<Vec<DbWorkflow>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::workflows
            .filter(dsl::deleted_at.is_null())
            .order(dsl::created_at.desc())
            .load::<DbWorkflow>(&mut conn)?)
    }

    pub fn update(
        &self,
        workflow_id: &str,
        input: DbUpdateWorkflow,
    ) -> Result<DbWorkflow, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let existing = dsl::workflows
            .filter(dsl::id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .first::<DbWorkflow>(&mut conn)
            .optional()?;

        let existing = match existing {
            Some(workflow) => workflow,
            None => return Err(DatabaseError::QueryError(diesel::result::Error::NotFound)),
        };

        if input.name.is_none()
            && input.objective.is_none()
            && input.eval_script.is_none()
            && input.state.is_none()
            && input.iteration_state.is_none()
        {
            return Ok(existing);
        }

        diesel::update(dsl::workflows)
            .filter(dsl::id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .set(&input)
            .execute(&mut conn)?;

        Ok(dsl::workflows
            .filter(dsl::id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .first::<DbWorkflow>(&mut conn)?)
    }

    pub fn soft_delete(&self, workflow_id: &str) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let affected = diesel::update(dsl::workflows)
            .filter(dsl::id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .set((
                dsl::deleted_at.eq(Some(now.clone())),
                dsl::updated_at.eq(now),
            ))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }

        Ok(())
    }
}
