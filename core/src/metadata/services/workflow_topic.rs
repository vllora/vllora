use crate::metadata::error::DatabaseError;
use crate::metadata::models::knowledge_source::DbKnowledgeSource;
use crate::metadata::models::knowledge_source_part::DbKnowledgeSourcePart;
use crate::metadata::models::workflow_topic::{
    DbNewWorkflowTopic, DbWorkflowTopic, TopicUpdateInput,
};
use crate::metadata::models::workflow_topic_source::{
    DbNewWorkflowTopicSource, DbWorkflowTopicSource, TopicSourceCreateInput, TopicSourceUpdateInput,
};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::knowledge_source_parts::dsl as source_parts_dsl;
use crate::metadata::schema::knowledge_sources::dsl as knowledge_dsl;
use crate::metadata::schema::workflow_topic_sources::dsl as relation_dsl;
use crate::metadata::schema::workflow_topics::dsl;
use diesel::BoolExpressionMethods;
use diesel::Connection;
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
            .map(|t| {
                let mut t = t.with_defaults();
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
            .order(dsl::created_at.desc())
            .load::<DbWorkflowTopic>(&mut conn)?)
    }

    pub fn update_many(
        &self,
        workflow_id: &str,
        topics: Vec<TopicUpdateInput>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let mut updated = 0usize;

        conn.transaction::<(), DatabaseError, _>(|conn| {
            for topic in topics {
                let existing = dsl::workflow_topics
                    .filter(dsl::workflow_id.eq(workflow_id))
                    .filter(
                        dsl::id
                            .eq(&topic.identifier)
                            .or(dsl::reference_id.eq(&topic.identifier)),
                    )
                    .first::<DbWorkflowTopic>(conn)?;

                let changes = (
                    topic.name.as_ref().map(|v| dsl::name.eq(v)),
                    topic
                        .parent_id
                        .as_ref()
                        .map(|v| dsl::parent_id.eq(Some(v.clone()))),
                    topic
                        .system_prompt
                        .as_ref()
                        .map(|v| dsl::system_prompt.eq(v)),
                    topic
                        .reference_id
                        .as_ref()
                        .map(|v| dsl::reference_id.eq(Some(v.clone()))),
                );

                let rows = diesel::update(dsl::workflow_topics.filter(dsl::id.eq(existing.id)))
                    .set(changes)
                    .execute(conn)?;
                updated += rows;
            }
            Ok(())
        })?;

        Ok(updated)
    }

    pub fn delete_many(
        &self,
        workflow_id: &str,
        identifiers: Vec<String>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::delete(dsl::workflow_topics)
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(
                dsl::id
                    .eq_any(&identifiers)
                    .or(dsl::reference_id.eq_any(&identifiers)),
            )
            .execute(&mut conn)?;
        Ok(affected)
    }

    pub fn create_relations(
        &self,
        workflow_id: &str,
        relations: Vec<TopicSourceCreateInput>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let mut rows: Vec<DbNewWorkflowTopicSource> = Vec::with_capacity(relations.len());

        for relation in relations {
            let topic = dsl::workflow_topics
                .filter(dsl::workflow_id.eq(workflow_id))
                .filter(
                    dsl::id
                        .eq(&relation.topic_identifier)
                        .or(dsl::reference_id.eq(&relation.topic_identifier)),
                )
                .first::<DbWorkflowTopic>(&mut conn)?;
            let source_part = source_parts_dsl::knowledge_source_parts
                .filter(
                    source_parts_dsl::id
                        .eq(&relation.part_identifier)
                        .or(source_parts_dsl::reference_id.eq(&relation.part_identifier)),
                )
                .first::<DbKnowledgeSourcePart>(&mut conn)?;

            // Validate part belongs to a source in this workflow.
            knowledge_dsl::knowledge_sources
                .filter(knowledge_dsl::id.eq(&source_part.source_id))
                .filter(knowledge_dsl::workflow_id.eq(workflow_id))
                .filter(knowledge_dsl::deleted_at.is_null())
                .first::<DbKnowledgeSource>(&mut conn)?;

            rows.push(
                DbNewWorkflowTopicSource {
                    id: relation.id,
                    reference_id: relation.reference_id,
                    workflow_id: workflow_id.to_string(),
                    topic_id: topic.id,
                    source_part_id: source_part.id,
                }
                .with_defaults(),
            );
        }

        let count = diesel::insert_into(relation_dsl::workflow_topic_sources)
            .values(&rows)
            .execute(&mut conn)?;
        Ok(count)
    }

    pub fn list_relations(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<DbWorkflowTopicSource>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(relation_dsl::workflow_topic_sources
            .filter(relation_dsl::workflow_id.eq(workflow_id))
            .order(relation_dsl::created_at.desc())
            .load::<DbWorkflowTopicSource>(&mut conn)?)
    }

    pub fn update_relations(
        &self,
        workflow_id: &str,
        relations: Vec<TopicSourceUpdateInput>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let mut updated = 0usize;

        conn.transaction::<(), DatabaseError, _>(|conn| {
            for rel in relations {
                let existing = relation_dsl::workflow_topic_sources
                    .filter(relation_dsl::workflow_id.eq(workflow_id))
                    .filter(
                        relation_dsl::id
                            .eq(&rel.identifier)
                            .or(relation_dsl::reference_id.eq(&rel.identifier)),
                    )
                    .first::<DbWorkflowTopicSource>(conn)?;

                let topic_id = if let Some(topic_identifier) = rel.topic_identifier.as_ref() {
                    Some(
                        dsl::workflow_topics
                            .filter(dsl::workflow_id.eq(workflow_id))
                            .filter(
                                dsl::id
                                    .eq(topic_identifier)
                                    .or(dsl::reference_id.eq(topic_identifier)),
                            )
                            .first::<DbWorkflowTopic>(conn)?
                            .id,
                    )
                } else {
                    None
                };

                let source_part_id = if let Some(part_identifier) = rel.part_identifier.as_ref() {
                    let part = source_parts_dsl::knowledge_source_parts
                        .filter(
                            source_parts_dsl::id
                                .eq(part_identifier)
                                .or(source_parts_dsl::reference_id.eq(part_identifier)),
                        )
                        .first::<DbKnowledgeSourcePart>(conn)?;
                    knowledge_dsl::knowledge_sources
                        .filter(knowledge_dsl::id.eq(&part.source_id))
                        .filter(knowledge_dsl::workflow_id.eq(workflow_id))
                        .filter(knowledge_dsl::deleted_at.is_null())
                        .first::<DbKnowledgeSource>(conn)?;
                    Some(part.id)
                } else {
                    None
                };

                let changes = (
                    rel.reference_id
                        .as_ref()
                        .map(|v| relation_dsl::reference_id.eq(Some(v.clone()))),
                    topic_id.as_ref().map(|v| relation_dsl::topic_id.eq(v)),
                    source_part_id
                        .as_ref()
                        .map(|v| relation_dsl::source_part_id.eq(v)),
                );

                let rows = diesel::update(
                    relation_dsl::workflow_topic_sources.filter(relation_dsl::id.eq(existing.id)),
                )
                .set(changes)
                .execute(conn)?;
                updated += rows;
            }
            Ok(())
        })?;

        Ok(updated)
    }

    pub fn delete_relations(
        &self,
        workflow_id: &str,
        identifiers: Vec<String>,
    ) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::delete(relation_dsl::workflow_topic_sources)
            .filter(relation_dsl::workflow_id.eq(workflow_id))
            .filter(
                relation_dsl::id
                    .eq_any(&identifiers)
                    .or(relation_dsl::reference_id.eq_any(&identifiers)),
            )
            .execute(&mut conn)?;
        Ok(affected)
    }

    /// Replace all topics for a workflow in a single transaction (delete + insert).
    pub fn replace(
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

        conn.transaction::<_, DatabaseError, _>(|conn| {
            diesel::delete(dsl::workflow_topics)
                .filter(dsl::workflow_id.eq(workflow_id))
                .execute(conn)?;

            let count = diesel::insert_into(dsl::workflow_topics)
                .values(&topics)
                .execute(conn)?;

            Ok(count)
        })
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
    fn test_create_topics() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowTopicService::new(db_pool.clone());

        let topics = vec![DbNewWorkflowTopic {
            id: Some("t1".into()),
            reference_id: Some("topic-ref-1".into()),
            workflow_id: String::new(),
            name: "Root".into(),
            parent_id: None,
            system_prompt: Some("You are a topic assistant".into()),
        }];
        service.create(&wf_id, topics).unwrap();

        let result = service.list(&wf_id).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].reference_id, Some("topic-ref-1".to_string()));
    }

    #[test]
    fn test_delete_topics() {
        let db_pool = setup_test_database();
        let wf_id = create_test_workflow(&db_pool);
        let service = WorkflowTopicService::new(db_pool.clone());

        service
            .create(
                &wf_id,
                vec![DbNewWorkflowTopic {
                    id: Some("t1".into()),
                    reference_id: Some("topic-ref-1".into()),
                    workflow_id: String::new(),
                    name: "Root".into(),
                    parent_id: None,
                    system_prompt: None,
                }],
            )
            .unwrap();

        service
            .delete_many(&wf_id, vec!["topic-ref-1".into()])
            .unwrap();
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
                    id: Some("t1".into()),
                    reference_id: None,
                    workflow_id: String::new(),
                    name: "A".into(),
                    parent_id: None,
                    system_prompt: None,
                }],
            )
            .unwrap();
        service
            .create(
                &wf2,
                vec![DbNewWorkflowTopic {
                    id: Some("t2".into()),
                    reference_id: None,
                    workflow_id: String::new(),
                    name: "B".into(),
                    parent_id: None,
                    system_prompt: None,
                }],
            )
            .unwrap();

        service.delete_many(&wf1, vec!["t1".into()]).unwrap();
        assert_eq!(service.list(&wf1).unwrap().len(), 0);
        assert_eq!(service.list(&wf2).unwrap().len(), 1);
    }
}
