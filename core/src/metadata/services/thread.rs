use crate::metadata::error::DatabaseError;
use crate::metadata::pool::DbPool;
use diesel::sql_types::Float;
use diesel::QueryableByName;
use diesel::{sql_query, RunQueryDsl};

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
    #[cfg_attr(feature = "sqlite", diesel(sql_type = Float))]
    #[cfg_attr(feature = "postgres", diesel(sql_type = Float))]
    pub cost: f32,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub title: Option<String>,
}

#[derive(QueryableByName, Debug)]
pub struct ThreadTitleResult {
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub thread_id: String,
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

    pub fn list_thread_spans(
        &self,
        project_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ThreadSpanQueryResult>, DatabaseError> {
        let mut conn = self.db_pool.get()?;
        let query_sql = Self::build_thread_span_query(Some("?"), true);

        let spans = sql_query(&query_sql)
            .bind::<diesel::sql_types::Text, _>(project_id) // attribute.title subquery
            .bind::<diesel::sql_types::BigInt, _>(limit)
            .bind::<diesel::sql_types::BigInt, _>(offset)
            .load::<ThreadSpanQueryResult>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(spans)
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
            first_value(titles) over (PARTITION BY thread_id) title
        FROM (
            SELECT
                thread_id,
                COALESCE(
                    MIN(CASE WHEN parent_span_id IS NULL THEN start_time_us END),
                    MIN(start_time_us)
                ) as start_time_us,
                COALESCE(
                    MAX(CASE WHEN parent_span_id IS NULL THEN finish_time_us END),
                    MAX(finish_time_us)
                ) as finish_time_us,
                GROUP_CONCAT(DISTINCT CASE WHEN parent_span_id IS NULL THEN run_id END) as run_ids,
                GROUP_CONCAT(DISTINCT json_extract(attribute, '$.model_name')) as input_models,
                SUM(CAST(json_extract(attribute, '$.cost') AS REAL)) as cost,
                GROUP_CONCAT(json_extract(attribute, '$.title')) as titles
            FROM traces as main_traces
            WHERE project_id = {}
                AND thread_id IS NOT NULL
            GROUP BY thread_id
            HAVING start_time_us IS NOT NULL
            
        ) {}
        "#,
            where_filter, pagination
        )
    }
}
