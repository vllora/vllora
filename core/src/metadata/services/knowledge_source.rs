use crate::metadata::error::DatabaseError;
use crate::metadata::models::knowledge_source::{DbKnowledgeSource, DbNewKnowledgeSource};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::knowledge_sources::dsl;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;

pub struct KnowledgeSourceService {
    db_pool: DbPool,
}

impl KnowledgeSourceService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn create(
        &self,
        workflow_id: &str,
        name: &str,
        source_type: &str,
        content: Option<&str>,
        extracted_content: Option<&str>,
    ) -> Result<DbKnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let input = DbNewKnowledgeSource::new(
            workflow_id.to_string(),
            name.to_string(),
            source_type.to_string(),
            content.map(|s| s.to_string()),
            extracted_content.map(|s| s.to_string()),
        );
        let ks_id = input.id.clone();

        diesel::insert_into(dsl::knowledge_sources)
            .values(&input)
            .execute(&mut conn)?;

        Ok(dsl::knowledge_sources
            .filter(dsl::id.eq(ks_id))
            .first::<DbKnowledgeSource>(&mut conn)?)
    }

    pub fn get(&self, ks_id: &str) -> Result<DbKnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::id.eq(ks_id))
            .filter(dsl::deleted_at.is_null())
            .first::<DbKnowledgeSource>(&mut conn)?)
    }

    pub fn get_including_deleted(
        &self,
        ks_id: &str,
    ) -> Result<DbKnowledgeSource, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::id.eq(ks_id))
            .first::<DbKnowledgeSource>(&mut conn)?)
    }

    pub fn list(&self, workflow_id: &str) -> Result<Vec<DbKnowledgeSource>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .order(dsl::created_at.desc())
            .load::<DbKnowledgeSource>(&mut conn)?)
    }

    pub fn count(&self, workflow_id: &str) -> Result<i64, DatabaseError> {
        use diesel::dsl::count_star;
        let mut conn = self.db_pool.get()?;
        Ok(dsl::knowledge_sources
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .select(count_star())
            .first::<i64>(&mut conn)?)
    }

    pub fn update_status(&self, ks_id: &str, status: &str) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::update(dsl::knowledge_sources)
            .filter(dsl::id.eq(ks_id))
            .set(dsl::status.eq(status))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(())
    }

    pub fn update_extracted_content(
        &self,
        ks_id: &str,
        extracted_content: &str,
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let affected = diesel::update(dsl::knowledge_sources)
            .filter(dsl::id.eq(ks_id))
            .set(dsl::extracted_content.eq(Some(extracted_content)))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(())
    }

    pub fn soft_delete(&self, ks_id: &str) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let affected = diesel::update(dsl::knowledge_sources)
            .filter(dsl::id.eq(ks_id))
            .filter(dsl::deleted_at.is_null())
            .set(dsl::deleted_at.eq(Some(now)))
            .execute(&mut conn)?;

        if affected == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }
        Ok(())
    }

    pub fn soft_delete_all(&self, workflow_id: &str) -> Result<usize, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let affected = diesel::update(dsl::knowledge_sources)
            .filter(dsl::workflow_id.eq(workflow_id))
            .filter(dsl::deleted_at.is_null())
            .set(dsl::deleted_at.eq(Some(now)))
            .execute(&mut conn)?;
        Ok(affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_utils::setup_test_database;

    #[test]
    fn test_create_and_list() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        service.create("wf1", "doc.pdf", "pdf", None, None).unwrap();
        service
            .create("wf1", "notes.md", "markdown", None, None)
            .unwrap();

        let sources = service.list("wf1").unwrap();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn test_soft_delete_hides_from_list() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        let ks = service.create("wf1", "doc.pdf", "pdf", None, None).unwrap();
        service.soft_delete(&ks.id).unwrap();

        let sources = service.list("wf1").unwrap();
        assert_eq!(sources.len(), 0);
    }

    #[test]
    fn test_soft_delete_data_still_exists() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        let ks = service.create("wf1", "doc.pdf", "pdf", None, None).unwrap();
        service.soft_delete(&ks.id).unwrap();

        let fetched = service.get_including_deleted(&ks.id).unwrap();
        assert!(fetched.deleted_at.is_some());
        assert_eq!(fetched.name, "doc.pdf");
    }

    #[test]
    fn test_update_extracted_content() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        let ks = service.create("wf1", "doc.pdf", "pdf", None, None).unwrap();
        let content = r#"{"chunks":[{"id":"c1","title":"Intro","content":"Hello"}]}"#;
        service.update_extracted_content(&ks.id, content).unwrap();
        service.update_status(&ks.id, "ready").unwrap();

        let fetched = service.get(&ks.id).unwrap();
        assert_eq!(fetched.status, "ready");
        assert!(fetched.extracted_content.is_some());
    }

    #[test]
    fn test_count() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        service.create("wf1", "a.pdf", "pdf", None, None).unwrap();
        service.create("wf1", "b.pdf", "pdf", None, None).unwrap();
        let ks3 = service.create("wf1", "c.pdf", "pdf", None, None).unwrap();
        service.soft_delete(&ks3.id).unwrap();

        assert_eq!(service.count("wf1").unwrap(), 2);
    }

    #[test]
    fn test_soft_delete_all() {
        let db_pool = setup_test_database();
        let service = KnowledgeSourceService::new(db_pool.clone());

        service.create("wf1", "a.pdf", "pdf", None, None).unwrap();
        service.create("wf1", "b.pdf", "pdf", None, None).unwrap();
        service.create("wf2", "c.pdf", "pdf", None, None).unwrap();

        service.soft_delete_all("wf1").unwrap();

        assert_eq!(service.count("wf1").unwrap(), 0);
        assert_eq!(service.count("wf2").unwrap(), 1);
    }
}
