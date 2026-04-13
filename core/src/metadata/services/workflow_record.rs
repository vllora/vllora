use crate::metadata::error::DatabaseError;
use crate::metadata::models::workflow_record::{
    DbNewWorkflowRecord, DbNewWorkflowRecordScore, DbWorkflowRecord, DbWorkflowRecordScore,
    RecordsSummary, TopicRecordCount,
};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::workflow_records::dsl;
use diesel::Connection;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct WorkflowRecordService {
    db_pool: DbPool,
}

impl WorkflowRecordService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn add(
        &self,
        workflow_id: &str,
        records: Vec<DbNewWorkflowRecord>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let records: Vec<DbNewWorkflowRecord> = records
            .into_iter()
            .map(|mut r| {
                r.workflow_id = workflow_id.to_string();
                r
            })
            .collect();

        // Upsert: ON CONFLICT update mutable fields so re-sent records
        // (e.g., with updated topic_id or data) are applied rather than rejected.
        // SQLite requires per-row inserts for on_conflict; wrap in a transaction.
        let mut total = 0;
        conn.transaction::<_, DatabaseError, _>(|conn| {
            for record in &records {
                let rows = diesel::insert_into(dsl::workflow_records)
                    .values(record)
                    .on_conflict((dsl::id, dsl::workflow_id))
                    .do_update()
                    .set((
                        dsl::data.eq(&record.data),
                        dsl::topic_id.eq(&record.topic_id),
                        dsl::span_id.eq(&record.span_id),
                        dsl::is_generated.eq(record.is_generated),
                        dsl::source_record_id.eq(&record.source_record_id),
                        dsl::metadata.eq(&record.metadata),
                    ))
                    .execute(conn)?;
                total += rows;
            }
            Ok(())
        })?;

        Ok(total)
    }

    pub fn list(&self, workflow_id: &str) -> Result<Vec<DbWorkflowRecord>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .order(dsl::created_at.desc())
            .load::<DbWorkflowRecord>(&mut conn)?)
    }

    pub fn count(&self, workflow_id: &str) -> Result<i64, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .count()
            .get_result(&mut conn)?)
    }

    /// Paginated list: returns (records, total_count).
    pub fn list_paginated(
        &self,
        workflow_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<DbWorkflowRecord>, i64), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let total: i64 = dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .count()
            .get_result(&mut conn)?;
        let records = dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .order(dsl::created_at.desc())
            .limit(limit)
            .offset(offset)
            .load::<DbWorkflowRecord>(&mut conn)?;
        Ok((records, total))
    }

    /// Paginated list filtered by topic_id: returns (records, total_count).
    pub fn list_paginated_by_topic(
        &self,
        workflow_id: &str,
        topic_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<DbWorkflowRecord>, i64), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let total: i64 = dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::topic_id.eq(topic_id))
            .count()
            .get_result(&mut conn)?;
        let records = dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::topic_id.eq(topic_id))
            .order(dsl::created_at.desc())
            .limit(limit)
            .offset(offset)
            .load::<DbWorkflowRecord>(&mut conn)?;
        Ok((records, total))
    }

    /// Lightweight aggregate stats — no row data transferred.
    pub fn summary(&self, workflow_id: &str) -> Result<RecordsSummary, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let total: i64 = dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .count()
            .get_result(&mut conn)?;
        let with_topic: i64 = dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::topic_id.is_not_null())
            .count()
            .get_result(&mut conn)?;
        let generated: i64 = dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::is_generated.eq(1))
            .count()
            .get_result(&mut conn)?;
        Ok(RecordsSummary { total, with_topic, generated })
    }

    /// Record counts grouped by topic_id — no row data transferred.
    pub fn counts_by_topic(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<TopicRecordCount>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let results = diesel::sql_query(
            "SELECT topic_id, COUNT(*) as count \
             FROM workflow_records \
             WHERE workflow_id = ? AND topic_id IS NOT NULL \
             GROUP BY topic_id",
        )
        .bind::<diesel::sql_types::Text, _>(workflow_id)
        .load::<TopicRecordCount>(&mut conn)
        .map_err(DatabaseError::QueryError)?;
        Ok(results)
    }

    /// Check if a span_id exists without loading all records.
    pub fn span_exists(&self, workflow_id: &str, span_id: &str) -> Result<bool, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let count: i64 = dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::span_id.eq(span_id))
            .count()
            .get_result(&mut conn)?;
        Ok(count > 0)
    }

    pub fn replace_all(
        &self,
        workflow_id: &str,
        records: Vec<DbNewWorkflowRecord>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Delete associated scores first
        diesel::delete(score_dsl::workflow_record_scores)
            .filter(score_dsl::workflow_id.eq(workflow_id))
            .execute(&mut conn)?;

        // Delete all existing records for this workflow
        diesel::delete(dsl::workflow_records)
            .filter(dsl::workflow_id.eq(workflow_id))
            .execute(&mut conn)?;

        // Insert new records
        let records: Vec<DbNewWorkflowRecord> = records
            .into_iter()
            .map(|mut r| {
                r.workflow_id = workflow_id.to_string();
                r
            })
            .collect();

        let count = diesel::insert_into(dsl::workflow_records)
            .values(&records)
            .execute(&mut conn)?;

        Ok(count)
    }

    pub fn update_topic(
        &self,
        workflow_id: &str,
        record_id: &str,
        topic_id: Option<&str>,
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::update(dsl::workflow_records)
            .filter(dsl::id.eq(record_id))
            .filter(dsl::workflow_id.eq(workflow_id))
            .set(dsl::topic_id.eq(topic_id))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(())
    }

    pub fn batch_update_topics(
        &self,
        workflow_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        for (record_id, topic_id) in updates {
            diesel::update(dsl::workflow_records)
                .filter(dsl::id.eq(record_id))
                .filter(dsl::workflow_id.eq(workflow_id))
                .set(dsl::topic_id.eq(Some(*topic_id)))
                .execute(&mut conn)?;
        }
        Ok(())
    }

    pub fn update_data(
        &self,
        workflow_id: &str,
        record_id: &str,
        data: &str,
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::update(dsl::workflow_records)
            .filter(dsl::id.eq(record_id))
            .filter(dsl::workflow_id.eq(workflow_id))
            .set(dsl::data.eq(data))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(())
    }

    pub fn rename_topic(
        &self,
        workflow_id: &str,
        old_topic_id: &str,
        new_topic_id: &str,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::update(dsl::workflow_records)
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::topic_id.eq(old_topic_id))
            .set(dsl::topic_id.eq(new_topic_id))
            .execute(&mut conn)?;
        Ok(affected)
    }

    pub fn clear_topic(&self, workflow_id: &str, topic_id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::update(dsl::workflow_records)
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::topic_id.eq(topic_id))
            .set(dsl::topic_id.eq(None::<String>))
            .execute(&mut conn)?;
        Ok(affected)
    }

    pub fn clear_all_topics(&self, workflow_id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::update(dsl::workflow_records)
            .filter(dsl::workflow_id.eq(workflow_id))
            .set(dsl::topic_id.eq(None::<String>))
            .execute(&mut conn)?;
        Ok(affected)
    }

    pub fn delete(&self, workflow_id: &str, record_id: &str) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Delete associated scores first
        diesel::delete(score_dsl::workflow_record_scores)
            .filter(score_dsl::record_id.eq(record_id))
            .filter(score_dsl::workflow_id.eq(workflow_id))
            .execute(&mut conn)?;

        let affected = diesel::delete(dsl::workflow_records)
            .filter(dsl::id.eq(record_id))
            .filter(dsl::workflow_id.eq(workflow_id))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(())
    }

    pub fn delete_all(&self, workflow_id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        // Delete associated scores first
        diesel::delete(score_dsl::workflow_record_scores)
            .filter(score_dsl::workflow_id.eq(workflow_id))
            .execute(&mut conn)?;

        let affected = diesel::delete(dsl::workflow_records)
            .filter(dsl::workflow_id.eq(workflow_id))
            .execute(&mut conn)?;
        Ok(affected)
    }
}

// =============================================================================
// WorkflowRecordScoreService
// =============================================================================

use crate::metadata::schema::workflow_record_scores::dsl as score_dsl;

pub struct WorkflowRecordScoreService {
    db_pool: DbPool,
}

impl WorkflowRecordScoreService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    /// Upsert scores for a batch of records from a specific job.
    /// Uses UPDATE/INSERT on the (workflow_id, record_id, job_id, score_type) unique constraint.
    pub fn batch_upsert(
        &self,
        workflow_id: &str,
        job_id: &str,
        score_type: &str,
        updates: &[(String, f32)],
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let mut count = 0;

        for (record_id, score) in updates {
            // Try to update existing row first
            let affected = diesel::update(score_dsl::workflow_record_scores)
                .filter(score_dsl::workflow_id.eq(workflow_id))
                .filter(score_dsl::record_id.eq(record_id))
                .filter(score_dsl::job_id.eq(job_id))
                .filter(score_dsl::score_type.eq(score_type))
                .set(score_dsl::score.eq(*score))
                .execute(&mut conn)?;

            if affected == 0 {
                // Insert new row
                let new_score = DbNewWorkflowRecordScore {
                    id: uuid::Uuid::new_v4().to_string(),
                    record_id: record_id.clone(),
                    workflow_id: workflow_id.to_string(),
                    job_id: job_id.to_string(),
                    score_type: score_type.to_string(),
                    score: *score,
                };
                diesel::insert_into(score_dsl::workflow_record_scores)
                    .values(&new_score)
                    .execute(&mut conn)?;
            }
            count += 1;
        }

        Ok(count)
    }

    /// List all scores for a workflow.
    pub fn list_by_workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<DbWorkflowRecordScore>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(score_dsl::workflow_record_scores
            .filter(score_dsl::workflow_id.eq(workflow_id))
            .order(score_dsl::created_at.desc())
            .load::<DbWorkflowRecordScore>(&mut conn)?)
    }

    /// List all scores for a specific record.
    pub fn list_by_record(
        &self,
        workflow_id: &str,
        record_id: &str,
    ) -> Result<Vec<DbWorkflowRecordScore>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(score_dsl::workflow_record_scores
            .filter(score_dsl::workflow_id.eq(workflow_id))
            .filter(score_dsl::record_id.eq(record_id))
            .order(score_dsl::created_at.desc())
            .load::<DbWorkflowRecordScore>(&mut conn)?)
    }

    /// Delete all scores for a workflow.
    pub fn delete_all(&self, workflow_id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::delete(score_dsl::workflow_record_scores)
            .filter(score_dsl::workflow_id.eq(workflow_id))
            .execute(&mut conn)?;
        Ok(affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::models::workflow::DbNewWorkflow;
    use crate::metadata::models::workflow_topic::DbNewWorkflowTopic;
    use crate::metadata::services::workflow::WorkflowService;
    use crate::metadata::services::workflow_topic::WorkflowTopicService;
    use crate::metadata::test_utils::setup_test_database;

    fn create_test_workflow(db_pool: &DbPool) -> String {
        let service = WorkflowService::new(db_pool.clone());
        let wf = service
            .create(DbNewWorkflow::new("test".into(), "obj".into()))
            .unwrap();
        wf.id
    }

    fn make_record(id: &str, topic_id: Option<&str>) -> DbNewWorkflowRecord {
        DbNewWorkflowRecord {
            id: id.to_string(),
            workflow_id: String::new(),
            data: r#"{"input":{},"output":{}}"#.to_string(),
            topic_id: topic_id.map(|t| t.to_string()),
            span_id: None,
            is_generated: 0,
            source_record_id: None,
            metadata: None,
        }
    }

    fn create_topic(db_pool: &DbPool, workflow_id: &str, topic_id: &str) {
        let topic_service = WorkflowTopicService::new(db_pool.clone());
        topic_service
            .create(
                workflow_id,
                vec![DbNewWorkflowTopic {
                    id: Some(topic_id.to_string()),
                    reference_id: None,
                    workflow_id: String::new(),
                    name: topic_id.to_string(),
                    parent_id: None,
                    system_prompt: None,
                }],
            )
            .unwrap();
    }

    #[test]
    fn test_add_and_list_records() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());
        create_topic(&db_pool, &wf_id, "greetings");

        let records = vec![
            make_record("r1", Some("greetings")),
            make_record("r2", None),
        ];
        service.add(&wf_id, records).unwrap();

        let result = service.list(&wf_id).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_replace_all_records() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());

        service
            .add(
                &wf_id,
                vec![make_record("r1", None), make_record("r2", None)],
            )
            .unwrap();
        assert_eq!(service.list(&wf_id).unwrap().len(), 2);

        service
            .replace_all(
                &wf_id,
                vec![
                    make_record("r10", None),
                    make_record("r11", None),
                    make_record("r12", None),
                ],
            )
            .unwrap();
        let result = service.list(&wf_id).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.iter().all(|r| r.id.starts_with("r1")));
    }

    #[test]
    fn test_update_topic() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());
        create_topic(&db_pool, &wf_id, "old");
        create_topic(&db_pool, &wf_id, "new_topic");

        service
            .add(&wf_id, vec![make_record("r1", Some("old"))])
            .unwrap();

        service
            .update_topic(&wf_id, "r1", Some("new_topic"))
            .unwrap();

        let records = service.list(&wf_id).unwrap();
        assert_eq!(records[0].topic_id, Some("new_topic".to_string()));
    }

    #[test]
    fn test_batch_update_topics() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());
        create_topic(&db_pool, &wf_id, "topicA");
        create_topic(&db_pool, &wf_id, "topicB");

        service
            .add(
                &wf_id,
                vec![
                    make_record("r1", None),
                    make_record("r2", None),
                    make_record("r3", None),
                ],
            )
            .unwrap();

        service
            .batch_update_topics(&wf_id, &[("r1", "topicA"), ("r2", "topicB")])
            .unwrap();

        let records = service.list(&wf_id).unwrap();
        let r1 = records.iter().find(|r| r.id == "r1").unwrap();
        let r2 = records.iter().find(|r| r.id == "r2").unwrap();
        let r3 = records.iter().find(|r| r.id == "r3").unwrap();
        assert_eq!(r1.topic_id, Some("topicA".to_string()));
        assert_eq!(r2.topic_id, Some("topicB".to_string()));
        assert_eq!(r3.topic_id, None);
    }

    #[test]
    fn test_rename_topic() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());
        create_topic(&db_pool, &wf_id, "old_name");
        create_topic(&db_pool, &wf_id, "new_name");
        create_topic(&db_pool, &wf_id, "other");

        service
            .add(
                &wf_id,
                vec![
                    make_record("r1", Some("old_name")),
                    make_record("r2", Some("old_name")),
                    make_record("r3", Some("other")),
                ],
            )
            .unwrap();

        service
            .rename_topic(&wf_id, "old_name", "new_name")
            .unwrap();

        let records = service.list(&wf_id).unwrap();
        let renamed: Vec<_> = records
            .iter()
            .filter(|r| r.topic_id.as_deref() == Some("new_name"))
            .collect();
        assert_eq!(renamed.len(), 2);
        let unchanged = records.iter().find(|r| r.id == "r3").unwrap();
        assert_eq!(unchanged.topic_id, Some("other".to_string()));
    }

    #[test]
    fn test_score_service_batch_upsert() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let record_service = WorkflowRecordService::new(db_pool.clone());
        let score_service = WorkflowRecordScoreService::new(db_pool.clone());

        record_service
            .add(
                &wf_id,
                vec![make_record("r1", None), make_record("r2", None)],
            )
            .unwrap();

        // Batch upsert eval scores
        score_service
            .batch_upsert(
                &wf_id,
                "job-1",
                "eval",
                &[("r1".to_string(), 0.85), ("r2".to_string(), 0.70)],
            )
            .unwrap();

        let scores = score_service.list_by_workflow(&wf_id).unwrap();
        assert_eq!(scores.len(), 2);
        let r1_score = scores.iter().find(|s| s.record_id == "r1").unwrap();
        assert_eq!(r1_score.score, 0.85);
        assert_eq!(r1_score.job_id, "job-1");
        assert_eq!(r1_score.score_type, "eval");

        // Upsert same job updates the score
        score_service
            .batch_upsert(&wf_id, "job-1", "eval", &[("r1".to_string(), 0.95)])
            .unwrap();

        let scores = score_service.list_by_workflow(&wf_id).unwrap();
        assert_eq!(scores.len(), 2); // still 2, not 3
        let r1_score = scores.iter().find(|s| s.record_id == "r1").unwrap();
        assert_eq!(r1_score.score, 0.95);

        // Different job creates a new entry
        score_service
            .batch_upsert(&wf_id, "job-2", "eval", &[("r1".to_string(), 0.60)])
            .unwrap();

        let scores = score_service.list_by_workflow(&wf_id).unwrap();
        assert_eq!(scores.len(), 3);

        // list_by_record returns both scores for r1
        let r1_scores = score_service.list_by_record(&wf_id, "r1").unwrap();
        assert_eq!(r1_scores.len(), 2);
    }

    #[test]
    fn test_delete_single() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());

        service
            .add(
                &wf_id,
                vec![make_record("r1", None), make_record("r2", None)],
            )
            .unwrap();

        service.delete(&wf_id, "r1").unwrap();
        assert_eq!(service.list(&wf_id).unwrap().len(), 1);
    }

    #[test]
    fn test_delete_all() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());

        service
            .add(
                &wf_id,
                vec![make_record("r1", None), make_record("r2", None)],
            )
            .unwrap();

        service.delete_all(&wf_id).unwrap();
        assert_eq!(service.list(&wf_id).unwrap().len(), 0);
    }

    #[test]
    fn test_clear_all_topics() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());
        create_topic(&db_pool, &wf_id, "a");
        create_topic(&db_pool, &wf_id, "b");

        service
            .add(
                &wf_id,
                vec![make_record("r1", Some("a")), make_record("r2", Some("b"))],
            )
            .unwrap();

        service.clear_all_topics(&wf_id).unwrap();

        let records = service.list(&wf_id).unwrap();
        assert!(records.iter().all(|r| r.topic_id.is_none()));
    }

    #[test]
    fn test_list_paginated() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());

        let records: Vec<_> = (0..10).map(|i| make_record(&format!("r{}", i), None)).collect();
        service.add(&wf_id, records).unwrap();

        let (page, total) = service.list_paginated(&wf_id, 3, 0).unwrap();
        assert_eq!(total, 10);
        assert_eq!(page.len(), 3);

        let (page2, total2) = service.list_paginated(&wf_id, 3, 3).unwrap();
        assert_eq!(total2, 10);
        assert_eq!(page2.len(), 3);

        // Past end
        let (page_end, _) = service.list_paginated(&wf_id, 5, 8).unwrap();
        assert_eq!(page_end.len(), 2);
    }

    #[test]
    fn test_summary() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());
        create_topic(&db_pool, &wf_id, "t1");

        service
            .add(
                &wf_id,
                vec![
                    make_record("r1", Some("t1")),
                    make_record("r2", None),
                    {
                        let mut r = make_record("r3", None);
                        r.is_generated = 1;
                        r
                    },
                ],
            )
            .unwrap();

        let summary = service.summary(&wf_id).unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.with_topic, 1);
        assert_eq!(summary.generated, 1);
    }

    #[test]
    fn test_span_exists() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());

        let mut record = make_record("r1", None);
        record.span_id = Some("span-abc".to_string());
        service.add(&wf_id, vec![record]).unwrap();

        assert!(service.span_exists(&wf_id, "span-abc").unwrap());
        assert!(!service.span_exists(&wf_id, "span-xyz").unwrap());
    }

    #[test]
    fn test_records_isolated_by_workflow() {
        let db_pool = setup_test_database();
        let wf1 = create_test_workflow(&db_pool);
        let wf2 = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());

        service.add(&wf1, vec![make_record("r1", None)]).unwrap();
        service.add(&wf2, vec![make_record("r2", None)]).unwrap();

        assert_eq!(service.list(&wf1).unwrap().len(), 1);
        assert_eq!(service.list(&wf2).unwrap().len(), 1);

        service.delete_all(&wf1).unwrap();
        assert_eq!(service.list(&wf1).unwrap().len(), 0);
        assert_eq!(service.list(&wf2).unwrap().len(), 1);
    }
}
