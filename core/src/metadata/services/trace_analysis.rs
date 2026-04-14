use crate::metadata::error::DatabaseError;
use crate::metadata::models::trace_analysis::{
    DbNewTraceAnalysis, DbTraceAnalysis, NewTraceAnalysis, TraceAnalysis,
};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::trace_analyses::dsl;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;
use diesel::SelectableHelper;

pub struct TraceAnalysisService {
    db_pool: DbPool,
}

impl TraceAnalysisService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    /// Get trace analysis for a workflow. Returns None if no analysis exists.
    pub fn get_by_workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Option<TraceAnalysis>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let result = dsl::trace_analyses
            .filter(dsl::workflow_id.eq(workflow_id))
            .select(DbTraceAnalysis::as_select())
            .first::<DbTraceAnalysis>(&mut conn);

        match result {
            Ok(row) => Ok(Some(TraceAnalysis::from_db(row))),
            Err(diesel::result::Error::NotFound) => Ok(None),
            Err(e) => Err(DatabaseError::from(e)),
        }
    }

    /// Upsert trace analysis for a workflow. If one already exists, replace it.
    pub fn upsert(
        &self,
        workflow_id: &str,
        input: NewTraceAnalysis,
    ) -> Result<TraceAnalysis, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Delete existing if any
        diesel::delete(dsl::trace_analyses.filter(dsl::workflow_id.eq(workflow_id)))
            .execute(&mut conn)?;

        // Insert new
        let db_new: DbNewTraceAnalysis = input.into_db_new(workflow_id.to_string());
        let analysis_id = db_new.id.clone();

        diesel::insert_into(dsl::trace_analyses)
            .values(&db_new)
            .execute(&mut conn)?;

        let row = dsl::trace_analyses
            .filter(dsl::id.eq(&analysis_id))
            .select(DbTraceAnalysis::as_select())
            .first::<DbTraceAnalysis>(&mut conn)?;

        Ok(TraceAnalysis::from_db(row))
    }
}
