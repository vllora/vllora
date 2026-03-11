use crate::metadata::error::DatabaseError;
use crate::metadata::models::knowledge::{DbKnowledge, DbNewKnowledge};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::knowledge::dsl;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct KnowledgeService {
    db_pool: DbPool,
}

impl KnowledgeService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create(&self, input: DbNewKnowledge) -> Result<DbKnowledge, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let knowledge_id = input
            .id
            .clone()
            .ok_or_else(|| DatabaseError::InvalidArgument("knowledge id is required".to_string()))?;

        diesel::insert_into(dsl::knowledge)
            .values(&input)
            .execute(&mut conn)?;

        Ok(dsl::knowledge
            .filter(dsl::id.eq(knowledge_id))
            .first::<DbKnowledge>(&mut conn)?)
    }

    pub fn list_by_workflow_id(&self, workflow_id: &str) -> Result<Vec<DbKnowledge>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge
            .filter(dsl::workflow_id.eq(workflow_id))
            .order(dsl::id.desc())
            .load::<DbKnowledge>(&mut conn)?)
    }
}
