use crate::metadata::error::DatabaseError;
use crate::metadata::models::thread::{DbNewThread, DbThread, DbUpdateThread, UpdateThreadDTO};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::threads;
use crate::types::threads::{MessageThread, MessageThreadWithTitle, PageOptions};
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::QueryableByName;
use diesel::{sql_query, QueryDsl, RunQueryDsl};
use std::collections::HashSet;

#[derive(QueryableByName, Debug)]
pub struct ThreadSpanQueryResult {
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub thread_id: String,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub start_time_us: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub finish_time_us: i64,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub run_ids: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub input_models: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Double)]
    pub cost: f64,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub title: Option<String>,
}

// For the efficient query approach, we'll use a struct that matches the SQL result
#[derive(QueryableByName, Debug, Clone)]
pub struct ThreadWithMessageInfo {
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub id: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub user_id: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub model_name: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub title: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub created_at: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub tenant_id: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub project_id: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    pub is_public: i32,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub description: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub keywords: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub last_message_at: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub model_names: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub cost: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub input_tokens: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub output_tokens: String,
}

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
            title: thread.title,
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
            is_public: update.is_public.map(|b| if b { 1 } else { 0 }),
            description: update.description,
            title: update.title,
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
        page_options: PageOptions,
    ) -> Result<Vec<MessageThreadWithTitle>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let limit = page_options.limit.unwrap_or(50);
        let offset = page_options.offset.unwrap_or(0);

        // Use subqueries to avoid row multiplication between messages and traces
        let sql_query_str = "SELECT 
                    t.*, 
                    m.last_message_at, 
                    tr.model_names, 
                    CAST(COALESCE(tr.cost, 0) AS TEXT) AS cost, 
                    CAST(COALESCE(tr.input_tokens, 0) AS TEXT) AS input_tokens, 
                    CAST(COALESCE(tr.output_tokens, 0) AS TEXT) AS output_tokens
                FROM threads t
                LEFT JOIN (
                    SELECT 
                        thread_id, 
                    strftime('%Y-%m-%d %H:%M:%S', (strftime('%s', MAX(created_at)) / 5) * 5, 'unixepoch') AS last_message_at
                    FROM messages
                    GROUP BY thread_id
                ) m ON t.id = m.thread_id
                LEFT JOIN (
                    SELECT 
                        thread_id,
                        group_concat(CASE WHEN operation_name = 'model_call' THEN json_extract(attribute, '$.model_name') END) AS model_names,
                        SUM(COALESCE(CAST(json_extract(attribute, '$.cost') AS REAL), 0)) AS cost,
                        SUM(CASE WHEN operation_name != 'model_call' THEN json_extract(json_extract(attribute, '$.usage'), '$.input_tokens') END) AS input_tokens,
                        SUM(CASE WHEN operation_name != 'model_call' THEN json_extract(json_extract(attribute, '$.usage'), '$.output_tokens') END) AS output_tokens
                    FROM traces
                    GROUP BY thread_id
                ) tr ON t.id = tr.thread_id
                WHERE t.project_id = ?
                ORDER BY last_message_at DESC NULLS LAST, t.created_at DESC
                LIMIT ? OFFSET ?".to_string();

        let query = sql_query(&sql_query_str);
        let results: Vec<ThreadWithMessageInfo> = query
            .bind::<diesel::sql_types::Text, _>(project_id)
            .bind::<diesel::sql_types::BigInt, _>(limit as i64)
            .bind::<diesel::sql_types::BigInt, _>(offset as i64)
            .load(&mut conn)?;

        Ok(results
            .into_iter()
            .map(|thread_info| {
                self.thread_with_message_info_to_message_thread_with_title(thread_info)
            })
            .collect())
    }

    pub fn count_threads_by_project(&self, project_id: &str) -> Result<i64, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        Ok(DbThread::by_project_id(project_id)
            .count()
            .get_result::<i64>(&mut conn)?)
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

    pub fn list_thread_spans(
        &self,
        project_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ThreadSpanQueryResult>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let query_sql = Self::build_thread_span_query(Some("?"), true);
        sql_query(&query_sql)
            .bind::<diesel::sql_types::Text, _>(project_id) // attribute.title subquery
            .bind::<diesel::sql_types::Text, _>(project_id) // message extraction subquery
            .bind::<diesel::sql_types::Text, _>(project_id) // main query WHERE
            .bind::<diesel::sql_types::BigInt, _>(limit)
            .bind::<diesel::sql_types::BigInt, _>(offset)
            .load(&mut conn)
            .map_err(DatabaseError::QueryError)
    }

    pub fn get_thread_span(
        &self,
        thread_id: &str,
        project_id: &str,
    ) -> Result<Option<ThreadSpanQueryResult>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let query_sql = Self::build_thread_span_query(Some("? AND thread_id = ?"), false);

        let results: Vec<ThreadSpanQueryResult> = sql_query(&query_sql)
            .bind::<diesel::sql_types::Text, _>(project_id) // attribute.title subquery
            .bind::<diesel::sql_types::Text, _>(project_id) // message extraction subquery
            .bind::<diesel::sql_types::Text, _>(project_id) // main query WHERE clause
            .bind::<diesel::sql_types::Text, _>(thread_id) // thread_id filter
            .load(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(results.into_iter().next())
    }

    pub fn count_thread_spans(&self, project_id: &str) -> Result<i64, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        #[derive(QueryableByName)]
        struct CountResult {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            count: i64,
        }

        let count_sql = r#"
            SELECT COUNT(DISTINCT thread_id) as count
            FROM traces
            WHERE project_id = ?
                AND thread_id IS NOT NULL
                AND parent_span_id IS NULL
        "#;

        let result = sql_query(count_sql)
            .bind::<diesel::sql_types::Text, _>(project_id)
            .load::<CountResult>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(result.first().map(|r| r.count).unwrap_or(0))
    }

    pub fn update_thread_title(
        &self,
        thread_id: &str,
        project_id: &str,
        title: &str,
    ) -> Result<(), DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let update_sql = r#"
            UPDATE traces
            SET attribute = json_set(COALESCE(attribute, '{}'), '$.title', ?)
            WHERE span_id = (
                SELECT span_id
                FROM traces
                WHERE thread_id = ?
                    AND project_id = ?
                ORDER BY start_time_us ASC
                LIMIT 1
            )
        "#;

        sql_query(update_sql)
            .bind::<diesel::sql_types::Text, _>(title)
            .bind::<diesel::sql_types::Text, _>(thread_id)
            .bind::<diesel::sql_types::Text, _>(project_id)
            .execute(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(())
    }

    fn build_thread_span_query(where_clause: Option<&str>, with_pagination: bool) -> String {
        let where_filter = where_clause.unwrap_or("?");
        let pagination = if with_pagination {
            "ORDER BY start_time_us DESC\n        LIMIT ? OFFSET ?"
        } else {
            ""
        };

        format!(
            r#"
        SELECT
            thread_id,
            start_time_us,
            finish_time_us,
            run_ids,
            input_models,
            COALESCE(cost, 0.0) as cost,
            title
        FROM (
            SELECT
                thread_id,
                MIN(CASE WHEN parent_span_id IS NULL THEN start_time_us END) as start_time_us,
                MAX(CASE WHEN parent_span_id IS NULL THEN finish_time_us END) as finish_time_us,
                GROUP_CONCAT(DISTINCT CASE WHEN parent_span_id IS NULL THEN run_id END) as run_ids,
                GROUP_CONCAT(DISTINCT json_extract(attribute, '$.model_name')) as input_models,
                SUM(CAST(json_extract(attribute, '$.cost') AS REAL)) as cost,
                (
                    SELECT COALESCE(
                        (
                            SELECT json_extract(attribute, '$.title')
                            FROM traces
                            WHERE thread_id = main_traces.thread_id
                                AND project_id = ?
                                AND json_extract(attribute, '$.title') IS NOT NULL
                            ORDER BY start_time_us ASC
                            LIMIT 1
                        ),
                        (
                            SELECT json_extract(value, '$.content')
                            FROM traces t_inner,
                                 json_each(json_extract(json(json_extract(t_inner.attribute, '$.request')), '$.messages'))
                            WHERE t_inner.thread_id = main_traces.thread_id
                                AND t_inner.project_id = ?
                                AND t_inner.operation_name = 'api_invoke'
                            ORDER BY
                                t_inner.start_time_us ASC,
                                CASE WHEN json_extract(value, '$.role') = 'user' THEN 0 ELSE 1 END
                            LIMIT 1
                        )
                    )
                ) as title
            FROM traces as main_traces
            WHERE project_id = {}
                AND thread_id IS NOT NULL
            GROUP BY thread_id
            HAVING start_time_us IS NOT NULL
        )
        {}
        "#,
            where_filter, pagination
        )
    }

    fn thread_with_message_info_to_message_thread_with_title(
        &self,
        thread_info: ThreadWithMessageInfo,
    ) -> MessageThreadWithTitle {
        let keywords = serde_json::from_str(&thread_info.keywords).unwrap_or_default();

        // Parse and deduplicate model names into input_models
        let input_models = if let Some(names) = thread_info.model_names.clone() {
            let mut seen = HashSet::new();
            let mut models: Vec<String> = Vec::new();
            for name in names.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                let candidate = name.to_string();
                if seen.insert(candidate.clone()) {
                    models.push(candidate);
                }
            }
            models
        } else {
            Vec::new()
        };

        MessageThreadWithTitle {
            id: thread_info.id,
            title: thread_info.title.unwrap_or("Untitled".to_string()),
            created_at: thread_info.created_at.clone(),
            updated_at: thread_info
                .last_message_at
                .unwrap_or(thread_info.created_at),
            input_models,
            mcp_template_definition_ids: vec![],
            cost: thread_info.cost.parse::<f64>().unwrap_or(0.0),
            input_tokens: thread_info.input_tokens.parse::<u64>().unwrap_or(0),
            output_tokens: thread_info.output_tokens.parse::<u64>().unwrap_or(0),
            description: thread_info.description,
            keywords: Some(keywords),
            is_public: thread_info.is_public != 0,
            project_id: thread_info.project_id.unwrap_or_default(),
            errors: None,
            tags_info: None,
            request_model_name: thread_info.model_name.unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::pool::DbPool;
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

    // Helper function to create a minimal test database pool
    fn create_mock_db_pool() -> DbPool {
        // Create an in-memory database for testing
        let test_db_path = ":memory:";
        let db_pool = crate::metadata::pool::establish_connection(test_db_path.to_string(), 5);
        crate::metadata::utils::init_db(&db_pool);
        db_pool
    }

    #[test]
    fn test_db_thread_to_message_thread() {
        let db_pool = create_mock_db_pool();
        let service = ThreadService::new(db_pool);

        let db_thread = DbThread {
            id: "test-id".to_string(),
            user_id: Some("user123".to_string()),
            model_name: Some("gpt-4".to_string()),
            title: Some("Test title".to_string()),
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
        let thread = create_test_thread();

        // This would require a real database connection to test
        // For now, just test the conversion logic
        let new_thread = DbNewThread {
            id: Some(thread.id.clone()),
            user_id: Some(thread.user_id),
            model_name: Some(thread.model_name),
            title: thread.title,
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

    #[test]
    fn test_list_threads_by_project_empty() {
        let db_pool = create_mock_db_pool();
        let service = ThreadService::new(db_pool);
        let page_options = PageOptions {
            limit: Some(50),
            offset: Some(0),
            ..Default::default()
        };

        let result = service.list_threads_by_project("non-existent-project", page_options);
        assert!(result.is_ok());
        let threads = result.unwrap();
        assert_eq!(threads.len(), 0);
    }

    #[test]
    fn test_thread_with_message_info_to_message_thread_with_title() {
        // Create a mock service without database dependency
        let service = ThreadService {
            db_pool: create_mock_db_pool(), // We won't use it in this test
        };

        let thread_info = ThreadWithMessageInfo {
            id: "test-thread".to_string(),
            user_id: Some("user-1".to_string()),
            model_name: Some("gpt-4".to_string()),
            title: Some("Untitled".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            tenant_id: Some("tenant-1".to_string()),
            project_id: Some("project-1".to_string()),
            is_public: 1,
            description: Some("Test description".to_string()),
            keywords: r#"["keyword1", "keyword2"]"#.to_string(),
            last_message_at: Some("2023-01-02T00:00:00Z".to_string()),
            model_names: Some("gpt-4,claude-3".to_string()),
            cost: "0.0".to_string(),
            input_tokens: "0".to_string(),
            output_tokens: "0".to_string(),
        };

        let result = service.thread_with_message_info_to_message_thread_with_title(thread_info);

        assert_eq!(result.id, "test-thread");
        assert_eq!(result.title, "Untitled");
        assert_eq!(result.created_at, "2023-01-01T00:00:00Z");
        assert_eq!(result.updated_at, "2023-01-02T00:00:00Z");
        assert_eq!(result.input_models, vec!["gpt-4", "claude-3"]);
        assert_eq!(result.description, Some("Test description".to_string()));
        assert_eq!(
            result.keywords,
            Some(vec!["keyword1".to_string(), "keyword2".to_string()])
        );
        assert_eq!(result.is_public, true);
        assert_eq!(result.project_id, "project-1");
        assert_eq!(result.request_model_name, "gpt-4");
        assert_eq!(result.cost, 0.0);
        assert_eq!(result.input_tokens, 0);
        assert_eq!(result.output_tokens, 0);
        assert_eq!(result.mcp_template_definition_ids.len(), 0);
        assert!(result.errors.is_none());
        assert!(result.tags_info.is_none());
    }

    #[test]
    fn test_thread_with_message_info_empty_human_models() {
        // Create a mock service without database dependency
        let service = ThreadService {
            db_pool: create_mock_db_pool(), // We won't use it in this test
        };

        let thread_info = ThreadWithMessageInfo {
            id: "test-thread".to_string(),
            user_id: Some("user-1".to_string()),
            model_name: Some("gpt-4".to_string()),
            title: Some("Untitled".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            tenant_id: Some("tenant-1".to_string()),
            project_id: Some("project-1".to_string()),
            is_public: 0,
            description: Some("Test description".to_string()),
            keywords: r#"[]"#.to_string(),
            last_message_at: None,
            model_names: None,
            cost: "0.0".to_string(),
            input_tokens: "0".to_string(),
            output_tokens: "0".to_string(),
        };

        let result = service.thread_with_message_info_to_message_thread_with_title(thread_info);

        assert_eq!(result.input_models.len(), 0);
        assert_eq!(result.updated_at, "2023-01-01T00:00:00Z"); // Should fallback to created_at
        assert_eq!(result.is_public, false);
    }

    #[test]
    fn test_thread_with_message_info_parsing_keywords() {
        // Create a mock service without database dependency
        let service = ThreadService {
            db_pool: create_mock_db_pool(), // We won't use it in this test
        };

        let thread_info = ThreadWithMessageInfo {
            id: "test-thread".to_string(),
            user_id: Some("user-1".to_string()),
            model_name: Some("gpt-4".to_string()),
            title: Some("Untitled".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            tenant_id: Some("tenant-1".to_string()),
            project_id: Some("project-1".to_string()),
            is_public: 0,
            description: Some("Test description".to_string()),
            keywords: r#"["tag1", "tag2", "tag3"]"#.to_string(),
            last_message_at: None,
            model_names: None,
            cost: "0.0".to_string(),
            input_tokens: "0".to_string(),
            output_tokens: "0".to_string(),
        };

        let result = service.thread_with_message_info_to_message_thread_with_title(thread_info);

        assert_eq!(
            result.keywords,
            Some(vec![
                "tag1".to_string(),
                "tag2".to_string(),
                "tag3".to_string()
            ])
        );
    }

    #[test]
    fn test_thread_with_message_info_human_model_parsing() {
        // Create a mock service without database dependency
        let service = ThreadService {
            db_pool: create_mock_db_pool(), // We won't use it in this test
        };

        let thread_info = ThreadWithMessageInfo {
            id: "test-thread".to_string(),
            user_id: Some("user-1".to_string()),
            model_name: Some("gpt-4".to_string()),
            title: Some("Untitled".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            tenant_id: Some("tenant-1".to_string()),
            project_id: Some("project-1".to_string()),
            is_public: 0,
            description: Some("Test description".to_string()),
            keywords: r#"[]"#.to_string(),
            last_message_at: None,
            model_names: Some("gpt-4,claude-3,gpt-3.5-turbo".to_string()),
            cost: "0.0".to_string(),
            input_tokens: "0".to_string(),
            output_tokens: "0".to_string(),
        };

        let result = service.thread_with_message_info_to_message_thread_with_title(thread_info);

        assert_eq!(
            result.input_models,
            vec!["gpt-4", "claude-3", "gpt-3.5-turbo"]
        );
    }
}
