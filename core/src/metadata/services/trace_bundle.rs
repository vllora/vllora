use crate::metadata::error::DatabaseError;
use crate::metadata::models::trace_bundle::{
    DbNewTraceBundle, DbTraceBundle, NewTraceBundle, TraceBundle,
};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::trace_bundles::dsl;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;
use diesel::SelectableHelper;

pub struct TraceBundleService {
    db_pool: DbPool,
}

impl TraceBundleService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    /// Store a new trace bundle. Rollup metadata (span_count, tool/model names)
    /// is derived from `input.semconv_spans` before insertion.
    pub fn create(&self, input: NewTraceBundle) -> Result<TraceBundle, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let db_new: DbNewTraceBundle = input.into_db_new()?;
        let bundle_id = db_new.id.clone();

        diesel::insert_into(dsl::trace_bundles)
            .values(&db_new)
            .execute(&mut conn)?;

        let row = dsl::trace_bundles
            .filter(dsl::id.eq(&bundle_id))
            .select(DbTraceBundle::as_select())
            .first::<DbTraceBundle>(&mut conn)?;

        Ok(TraceBundle::from_db(row)?)
    }

    pub fn get(&self, bundle_id: &str) -> Result<TraceBundle, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let row = dsl::trace_bundles
            .filter(dsl::id.eq(bundle_id))
            .select(DbTraceBundle::as_select())
            .first::<DbTraceBundle>(&mut conn)?;
        Ok(TraceBundle::from_db(row)?)
    }

    #[allow(dead_code)]
    pub fn list_by_workflow(&self, workflow_id: &str) -> Result<Vec<TraceBundle>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let rows = dsl::trace_bundles
            .filter(dsl::workflow_id.eq(workflow_id))
            .order(dsl::created_at.desc())
            .select(DbTraceBundle::as_select())
            .load::<DbTraceBundle>(&mut conn)?;
        rows.into_iter()
            .map(TraceBundle::from_db)
            .collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_utils::setup_test_database;
    use serde_json::json;

    #[test]
    fn create_then_get_roundtrips_blob_and_rollups() {
        let db_pool = setup_test_database();
        let service = TraceBundleService::new(db_pool);

        let spans = json!([
            {"gen_ai.request.model": "gpt-4o", "gen_ai.tool.name": "search"},
            {"gen_ai.response.model": "gpt-4o"},
        ]);

        let created = service
            .create(NewTraceBundle {
                workflow_id: "wf-1".to_string(),
                name: "trace-bundle-1".to_string(),
                semconv_spans: spans.clone(),
            })
            .unwrap();

        assert_eq!(created.workflow_id, "wf-1");
        assert_eq!(created.span_count, 2);
        assert_eq!(created.tool_names, vec!["search".to_string()]);
        assert_eq!(created.model_names, vec!["gpt-4o".to_string()]);
        assert_eq!(created.semconv_spans, spans);

        let fetched = service.get(&created.id).unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.semconv_spans, spans);
    }
}
