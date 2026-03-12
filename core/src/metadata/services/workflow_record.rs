use crate::metadata::error::DatabaseError;
use crate::metadata::models::workflow_record::{DbNewWorkflowRecord, DbWorkflowRecord};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::workflow_records::dsl;
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

        let count = diesel::insert_into(dsl::workflow_records)
            .values(&records)
            .execute(&mut conn)?;

        Ok(count)
    }

    pub fn list(&self, workflow_id: &str) -> Result<Vec<DbWorkflowRecord>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::workflow_records
            .filter(dsl::workflow_id.eq(workflow_id))
            .order(dsl::created_at.desc())
            .load::<DbWorkflowRecord>(&mut conn)?)
    }

    pub fn replace_all(
        &self,
        workflow_id: &str,
        records: Vec<DbNewWorkflowRecord>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;

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

    pub fn update_scores(
        &self,
        workflow_id: &str,
        record_id: &str,
        dry_run_score: Option<f32>,
        finetune_score: Option<f32>,
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::update(dsl::workflow_records)
            .filter(dsl::id.eq(record_id))
            .filter(dsl::workflow_id.eq(workflow_id))
            .set((
                dsl::dry_run_score.eq(dry_run_score),
                dsl::finetune_score.eq(finetune_score),
            ))
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
        let affected = diesel::delete(dsl::workflow_records)
            .filter(dsl::workflow_id.eq(workflow_id))
            .execute(&mut conn)?;
        Ok(affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::models::workflow::DbNewWorkflow;
    use crate::metadata::services::workflow::WorkflowService;
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

    #[test]
    fn test_add_and_list_records() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());

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
    fn test_update_scores() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowRecordService::new(db_pool.clone());

        service.add(&wf_id, vec![make_record("r1", None)]).unwrap();

        service
            .update_scores(&wf_id, "r1", Some(0.85), None)
            .unwrap();

        let records = service.list(&wf_id).unwrap();
        assert_eq!(records[0].dry_run_score, Some(0.85));
        assert_eq!(records[0].finetune_score, None);
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
