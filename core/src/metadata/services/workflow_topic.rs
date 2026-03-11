use crate::metadata::error::DatabaseError;
use crate::metadata::models::workflow_topic::{DbNewWorkflowTopic, DbWorkflowTopic};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::workflow_topics::dsl;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct WorkflowTopicService {
    db_pool: DbPool,
}

impl WorkflowTopicService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create(
        &self,
        workflow_id: &str,
        topics: Vec<DbNewWorkflowTopic>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let topics: Vec<DbNewWorkflowTopic> = topics
            .into_iter()
            .map(|mut t| {
                t.workflow_id = workflow_id.to_string();
                t
            })
            .collect();

        let count = diesel::insert_into(dsl::workflow_topics)
            .values(&topics)
            .execute(&mut conn)?;

        Ok(count)
    }

    pub fn list(&self, workflow_id: &str) -> Result<Vec<DbWorkflowTopic>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::workflow_topics
            .filter(dsl::workflow_id.eq(workflow_id))
            .load::<DbWorkflowTopic>(&mut conn)?)
    }

    pub fn delete_all(&self, workflow_id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::delete(dsl::workflow_topics)
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

    #[test]
    fn test_create_topic_tree() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowTopicService::new(db_pool.clone());

        let topics = vec![
            DbNewWorkflowTopic {
                id: "t1".into(),
                workflow_id: String::new(),
                name: "Root".into(),
                parent_id: None,
                selected: 1,
                source_chunk_refs: None,
            },
            DbNewWorkflowTopic {
                id: "t2".into(),
                workflow_id: String::new(),
                name: "Child A".into(),
                parent_id: Some("t1".into()),
                selected: 1,
                source_chunk_refs: Some(r#"["ks1:c1"]"#.into()),
            },
            DbNewWorkflowTopic {
                id: "t3".into(),
                workflow_id: String::new(),
                name: "Child B".into(),
                parent_id: Some("t1".into()),
                selected: 0,
                source_chunk_refs: None,
            },
        ];
        service.create(&wf_id, topics).unwrap();

        let result = service.list(&wf_id).unwrap();
        assert_eq!(result.len(), 3);

        let child_a = result.iter().find(|t| t.id == "t2").unwrap();
        assert_eq!(child_a.parent_id, Some("t1".to_string()));
        assert!(child_a.source_chunk_refs.is_some());
    }

    #[test]
    fn test_delete_all_topics() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowTopicService::new(db_pool.clone());

        service
            .create(
                &wf_id,
                vec![DbNewWorkflowTopic {
                    id: "t1".into(),
                    workflow_id: String::new(),
                    name: "Root".into(),
                    parent_id: None,
                    selected: 1,
                    source_chunk_refs: None,
                }],
            )
            .unwrap();

        service.delete_all(&wf_id).unwrap();
        assert_eq!(service.list(&wf_id).unwrap().len(), 0);
    }

    #[test]
    fn test_topics_isolated_by_workflow() {
        let db_pool = setup_test_database();
        let wf1 = create_test_workflow(&db_pool);
        let wf2 = create_test_workflow(&db_pool);
        let service = WorkflowTopicService::new(db_pool.clone());

        service
            .create(
                &wf1,
                vec![DbNewWorkflowTopic {
                    id: "t1".into(),
                    workflow_id: String::new(),
                    name: "A".into(),
                    parent_id: None,
                    selected: 1,
                    source_chunk_refs: None,
                }],
            )
            .unwrap();
        service
            .create(
                &wf2,
                vec![DbNewWorkflowTopic {
                    id: "t2".into(),
                    workflow_id: String::new(),
                    name: "B".into(),
                    parent_id: None,
                    selected: 1,
                    source_chunk_refs: None,
                }],
            )
            .unwrap();

        service.delete_all(&wf1).unwrap();
        assert_eq!(service.list(&wf1).unwrap().len(), 0);
        assert_eq!(service.list(&wf2).unwrap().len(), 1);
    }
}
