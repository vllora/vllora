use crate::metadata::error::DatabaseError;
use crate::metadata::pool::DbPool;
use diesel::prelude::*;
use diesel::{sql_query, RunQueryDsl};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeFilter {
    Model,
    Mcp,
}

#[derive(Debug, Clone)]
pub struct ListGroupQuery {
    pub project_id: Option<String>,
    pub thread_ids: Option<Vec<String>>,
    pub trace_ids: Option<Vec<String>>,
    pub model_name: Option<String>,
    pub type_filter: Option<TypeFilter>,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub bucket_size_seconds: i64, // Time bucket size in seconds
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize, Deserialize, QueryableByName)]
pub struct GroupUsageInformation {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub time_bucket: i64,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub thread_ids_json: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub trace_ids_json: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub run_ids_json: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub root_span_ids_json: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub request_models_json: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub used_models_json: String,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub llm_calls: i64,
    #[diesel(sql_type = diesel::sql_types::Double)]
    pub cost: f64,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>)]
    pub input_tokens: Option<i64>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>)]
    pub output_tokens: Option<i64>,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub start_time_us: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub finish_time_us: i64,
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub errors_json: String,
}

pub trait GroupService {
    fn list_root_group(
        &self,
        query: ListGroupQuery,
    ) -> Result<Vec<GroupUsageInformation>, DatabaseError>;
    fn count_root_group(&self, query: ListGroupQuery) -> Result<i64, DatabaseError>;
    fn get_by_time_bucket(
        &self,
        time_bucket: i64,
        bucket_size_seconds: i64,
        project_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::metadata::models::trace::DbTrace>, DatabaseError>;
    fn count_by_time_bucket(
        &self,
        time_bucket: i64,
        bucket_size_seconds: i64,
        project_id: Option<&str>,
    ) -> Result<i64, DatabaseError>;
}

pub struct GroupServiceImpl {
    db_pool: Arc<DbPool>,
}

impl GroupServiceImpl {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }

    // Helper function to safely escape string values for SQL
    fn escape_sql_string(value: &str) -> String {
        value.replace('\'', "''")
    }

    // Helper function to build a SQL IN clause with properly escaped values
    fn build_in_clause(values: &[String]) -> String {
        values
            .iter()
            .map(|v| format!("'{}'", Self::escape_sql_string(v)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn build_filters(&self, query: &ListGroupQuery) -> String {
        let mut conditions = vec![];

        if let Some(project_id) = &query.project_id {
            conditions.push(format!(
                "project_id = '{}'",
                Self::escape_sql_string(project_id)
            ));
        }

        if let Some(thread_ids) = &query.thread_ids {
            conditions.push(format!(
                "thread_id IN ({})",
                Self::build_in_clause(thread_ids)
            ));
        }

        if let Some(trace_ids) = &query.trace_ids {
            conditions.push(format!(
                "trace_id IN ({})",
                Self::build_in_clause(trace_ids)
            ));
        }

        if let Some(model_name) = &query.model_name {
            let escaped_model = Self::escape_sql_string(model_name);
            conditions.push(format!(
                "(operation_name = 'model_call' AND json_extract(attribute, '$.model_name') = '{}')
                 OR (operation_name = 'api_invoke' AND json_extract(json_extract(attribute, '$.request'), '$.model') = '{}')",
                escaped_model, escaped_model
            ));
        }

        if let Some(type_filter) = &query.type_filter {
            match type_filter {
                TypeFilter::Model => {
                    conditions.push("operation_name = 'model_call'".to_string());
                }
                TypeFilter::Mcp => {
                    conditions.push("operation_name = 'mcp_call'".to_string());
                }
            }
        }

        if let Some(start_min) = query.start_time_min {
            conditions.push(format!("start_time_us >= {}", start_min));
        }

        if let Some(start_max) = query.start_time_max {
            conditions.push(format!("start_time_us <= {}", start_max));
        }

        if conditions.is_empty() {
            String::new()
        } else {
            format!("AND {}", conditions.join(" AND "))
        }
    }
}

impl GroupService for GroupServiceImpl {
    fn list_root_group(
        &self,
        query: ListGroupQuery,
    ) -> Result<Vec<GroupUsageInformation>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let filter_clause = self.build_filters(&query);

        // Convert bucket size from seconds to microseconds
        let bucket_size_us = query.bucket_size_seconds * 1_000_000;

        // This query groups spans by time buckets based on start_time_us
        // Only considers spans that have at least run_id OR thread_id
        let sql_query_str = format!("SELECT
              time_bucket,
              COALESCE(json_group_array(DISTINCT thread_id) FILTER (WHERE thread_id IS NOT NULL), '[]') as thread_ids_json,
              COALESCE(json_group_array(DISTINCT trace_id), '[]') as trace_ids_json,
              COALESCE(json_group_array(DISTINCT run_id) FILTER (WHERE run_id IS NOT NULL), '[]') as run_ids_json,
              COALESCE(json_group_array(DISTINCT span_id), '[]') as root_span_ids_json,
              COALESCE(json_group_array(DISTINCT request_model) FILTER (WHERE request_model IS NOT NULL), '[]') as request_models_json,
              COALESCE(json_group_array(DISTINCT used_model) FILTER (WHERE used_model IS NOT NULL), '[]') as used_models_json,
              CAST(SUM(CASE WHEN operation_name = 'model_call' THEN 1 ELSE 0 END) AS BIGINT) as llm_calls,
              SUM(COALESCE(CAST(json_extract(attribute, '$.cost') as REAL), 0)) as cost,
              SUM(CASE WHEN operation_name != 'model_call' THEN json_extract(json_extract(attribute, '$.usage'), '$.input_tokens') END) AS input_tokens,
              SUM(CASE WHEN operation_name != 'model_call' THEN json_extract(json_extract(attribute, '$.usage'), '$.output_tokens') END) AS output_tokens,
              MIN(start_time_us) as start_time_us,
              MAX(finish_time_us) as finish_time_us,
              COALESCE(json_group_array(DISTINCT error_msg) FILTER (WHERE error_msg IS NOT NULL), '[]') as errors_json
            FROM (
              SELECT
                (start_time_us / {}) * {} as time_bucket,
                thread_id,
                trace_id,
                run_id,
                span_id,
                operation_name,
                CASE WHEN operation_name = 'api_invoke'
                  THEN json_extract(json_extract(attribute, '$.request'), '$.model')
                END as request_model,
                CASE WHEN operation_name = 'model_call'
                  THEN json_extract(attribute, '$.model_name')
                END as used_model,
                attribute,
                start_time_us,
                finish_time_us,
                json_extract(attribute, '$.error') as error_msg
              FROM traces
              WHERE (run_id IS NOT NULL OR thread_id IS NOT NULL)
                {}
            )
            GROUP BY time_bucket
            ORDER BY time_bucket DESC
            LIMIT ? OFFSET ?",
            bucket_size_us, bucket_size_us, filter_clause);
        tracing::info!("SQL Query: {}", sql_query_str);
        let results = sql_query(&sql_query_str)
            .bind::<diesel::sql_types::BigInt, _>(query.limit)
            .bind::<diesel::sql_types::BigInt, _>(query.offset)
            .load::<GroupUsageInformation>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(results)
    }

    fn count_root_group(&self, query: ListGroupQuery) -> Result<i64, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let filter_clause = self.build_filters(&query);

        // Convert bucket size from seconds to microseconds
        let bucket_size_us = query.bucket_size_seconds * 1_000_000;

        #[derive(QueryableByName)]
        struct CountResult {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            count: i64,
        }

        // Count distinct time buckets
        let sql_query_str = format!(
            "SELECT COUNT(DISTINCT (start_time_us / {}) * {}) as count
            FROM traces
            WHERE (run_id IS NOT NULL OR thread_id IS NOT NULL)
              {}",
            bucket_size_us, bucket_size_us, filter_clause
        );

        let result = sql_query(&sql_query_str)
            .load::<CountResult>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(result.first().map(|r| r.count).unwrap_or(0))
    }

    fn get_by_time_bucket(
        &self,
        time_bucket: i64,
        bucket_size_seconds: i64,
        project_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::metadata::models::trace::DbTrace>, DatabaseError> {
        use crate::metadata::schema::traces;

        let mut conn = self.db_pool.get()?;

        // Convert bucket size from seconds to microseconds
        let bucket_size_us = bucket_size_seconds * 1_000_000;

        // Calculate the time range for this bucket
        let bucket_start = time_bucket;
        let bucket_end = time_bucket + bucket_size_us;

        // Fetch ALL spans (not just root spans) in this time bucket
        // But exclude spans that have NEITHER run_id NOR thread_id
        let mut query = traces::table
            .filter(traces::start_time_us.ge(bucket_start))
            .filter(traces::start_time_us.lt(bucket_end))
            .filter(traces::run_id.is_not_null().or(traces::thread_id.is_not_null()))
            .into_boxed();

        // Apply project_id filter if provided
        if let Some(project_id) = project_id {
            query = query.filter(traces::project_id.eq(project_id));
        }

        let results = query
            .order(traces::start_time_us.asc())
            .limit(limit)
            .offset(offset)
            .load::<crate::metadata::models::trace::DbTrace>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(results)
    }

    fn count_by_time_bucket(
        &self,
        time_bucket: i64,
        bucket_size_seconds: i64,
        project_id: Option<&str>,
    ) -> Result<i64, DatabaseError> {
        use crate::metadata::schema::traces;
        use diesel::dsl::count_star;

        let mut conn = self.db_pool.get()?;

        // Convert bucket size from seconds to microseconds
        let bucket_size_us = bucket_size_seconds * 1_000_000;

        // Calculate the time range for this bucket
        let bucket_start = time_bucket;
        let bucket_end = time_bucket + bucket_size_us;

        // Count all spans (not just root spans) in this time bucket
        // But exclude spans that have NEITHER run_id NOR thread_id
        let mut query = traces::table
            .filter(traces::start_time_us.ge(bucket_start))
            .filter(traces::start_time_us.lt(bucket_end))
            .filter(traces::run_id.is_not_null().or(traces::thread_id.is_not_null()))
            .into_boxed();

        // Apply project_id filter if provided
        if let Some(project_id) = project_id {
            query = query.filter(traces::project_id.eq(project_id));
        }

        let count = query
            .select(count_star())
            .first::<i64>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(count)
    }
}
