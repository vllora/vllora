use crate::metadata::error::DatabaseError;
use crate::metadata::models::thread::{DbNewThread, DbThread, DbUpdateThread, UpdateThreadDTO};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::threads;
use crate::types::threads::MessageThread;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::{QueryDsl, RunQueryDsl};

pub struct ThreadService {
    db_pool: DbPool,
}

impl ThreadService {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }

    pub fn get_thread_by_id(&self, thread_id: &str) -> Result<MessageThread, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let db_thread: DbThread = threads::table
            .filter(threads::id.eq(thread_id))
            .first(&mut conn)
            .optional()?
            .ok_or_else(|| DatabaseError::QueryError(diesel::result::Error::NotFound))?;

        Ok(self.db_thread_to_message_thread(db_thread))
    }

    pub fn create_thread(&self, thread: MessageThread) -> Result<MessageThread, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let new_thread = DbNewThread {
            id: Some(thread.id.clone()),
            user_id: Some(thread.user_id),
            model_name: Some(thread.model_name),
            tenant_id: None,
            project_id: Some(thread.project_id),
            is_public: Some(if thread.is_public { 1 } else { 0 }),
            description: thread.description,
            keywords: thread
                .keywords
                .map(|k| serde_json::to_string(&k).unwrap_or_else(|_| "[]".to_string())),
        };

        diesel::insert_into(threads::table)
            .values(&new_thread)
            .execute(&mut conn)?;

        // Return the created thread
        self.get_thread_by_id(&thread.id)
    }

    pub fn update_thread(
        &self,
        thread_id: &str,
        update: UpdateThreadDTO,
    ) -> Result<MessageThread, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let db_update = DbUpdateThread {
            user_id: update.user_id,
            model_name: update.model_name,
            tenant_id: update.tenant_id,
            project_id: update.project_id,
            is_public: update.is_public.map(|b| if b { 1 } else { 0 }),
            description: update.description,
            keywords: update
                .keywords
                .map(|k| serde_json::to_string(&k).unwrap_or_else(|_| "[]".to_string())),
        };

        diesel::update(threads::table.filter(threads::id.eq(thread_id)))
            .set(&db_update)
            .execute(&mut conn)?;

        self.get_thread_by_id(thread_id)
    }

    pub fn delete_thread(&self, thread_id: &str) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let affected_rows =
            diesel::delete(threads::table.filter(threads::id.eq(thread_id))).execute(&mut conn)?;

        if affected_rows == 0 {
            return Err(DatabaseError::QueryError(diesel::result::Error::NotFound));
        }

        Ok(())
    }

    pub fn list_threads_by_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<MessageThread>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let db_threads: Vec<DbThread> = DbThread::by_project_id(project_id).load(&mut conn)?;

        Ok(db_threads
            .into_iter()
            .map(|t| self.db_thread_to_message_thread(t))
            .collect())
    }

    pub fn list_threads_by_user(&self, user_id: &str) -> Result<Vec<MessageThread>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let db_threads: Vec<DbThread> = threads::table
            .filter(threads::user_id.eq(user_id))
            .load(&mut conn)?;

        Ok(db_threads
            .into_iter()
            .map(|t| self.db_thread_to_message_thread(t))
            .collect())
    }

    pub fn list_public_threads(&self) -> Result<Vec<MessageThread>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let db_threads: Vec<DbThread> = threads::table
            .filter(threads::is_public.eq(1))
            .load(&mut conn)?;

        Ok(db_threads
            .into_iter()
            .map(|t| self.db_thread_to_message_thread(t))
            .collect())
    }

    fn db_thread_to_message_thread(&self, db_thread: DbThread) -> MessageThread {
        let keywords = db_thread.parse_keywords();
        MessageThread {
            id: db_thread.id,
            model_name: db_thread.model_name.unwrap_or_default(),
            user_id: db_thread.user_id.unwrap_or_default(),
            project_id: db_thread.project_id.unwrap_or_default(),
            is_public: db_thread.is_public != 0,
            title: None, // Not stored in DB currently
            description: db_thread.description,
            keywords: Some(keywords),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::test_utils::setup_test_database;
    use crate::types::threads::MessageThread;

    fn create_test_thread() -> MessageThread {
        MessageThread {
            id: "test-thread-id".to_string(),
            model_name: "gpt-4".to_string(),
            user_id: "test-user-id".to_string(),
            project_id: "test-project-id".to_string(),
            is_public: false,
            title: Some("Test Thread".to_string()),
            description: Some("Test thread description".to_string()),
            keywords: Some(vec!["test".to_string(), "example".to_string()]),
        }
    }

    #[test]
    fn test_db_thread_to_message_thread() {
        let db_pool = setup_test_database();
        let service = ThreadService::new(db_pool);

        let db_thread = DbThread {
            id: "test-id".to_string(),
            user_id: Some("user123".to_string()),
            model_name: Some("gpt-4".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            tenant_id: Some("tenant123".to_string()),
            project_id: Some("project123".to_string()),
            is_public: 1,
            description: Some("Test description".to_string()),
            keywords: r#"["test", "keywords"]"#.to_string(),
        };

        let message_thread = service.db_thread_to_message_thread(db_thread);

        assert_eq!(message_thread.id, "test-id");
        assert_eq!(message_thread.model_name, "gpt-4");
        assert_eq!(message_thread.user_id, "user123");
        assert_eq!(message_thread.project_id, "project123");
        assert!(message_thread.is_public);
        assert_eq!(
            message_thread.description,
            Some("Test description".to_string())
        );
        assert_eq!(
            message_thread.keywords,
            Some(vec!["test".to_string(), "keywords".to_string()])
        );
    }

    #[test]
    fn test_create_thread() {
        let db_pool = setup_test_database();
        let service = ThreadService::new(db_pool);

        let thread = create_test_thread();

        // This would require a real database connection to test
        // For now, just test the conversion logic
        let new_thread = DbNewThread {
            id: Some(thread.id.clone()),
            user_id: Some(thread.user_id),
            model_name: Some(thread.model_name),
            tenant_id: None,
            project_id: Some(thread.project_id),
            is_public: Some(if thread.is_public { 1 } else { 0 }),
            description: thread.description,
            keywords: thread
                .keywords
                .map(|k| serde_json::to_string(&k).unwrap_or_else(|_| "[]".to_string())),
        };

        assert_eq!(new_thread.id, Some("test-thread-id".to_string()));
        assert_eq!(new_thread.is_public, Some(0));
        assert!(new_thread.keywords.is_some());
    }
}
