use crate::metadata::error::DatabaseError;
use crate::metadata::models::run::RunUsageInformation;
use crate::metadata::pool::DbPool;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeFilter {
    Model,
    Mcp,
}

#[derive(Debug, Clone)]
pub struct ListRunsQuery {
    pub project_id: Option<String>,
    pub run_ids: Option<Vec<String>>,
    pub thread_ids: Option<Vec<String>>,
    pub trace_ids: Option<Vec<String>>,
    pub model_name: Option<String>,
    pub type_filter: Option<TypeFilter>,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub limit: i64,
    pub offset: i64,
    pub include_mcp_templates: bool,
}

pub trait RunService {
    fn list(&self, query: ListRunsQuery) -> Result<Vec<RunUsageInformation>, DatabaseError>;
    fn count(&self, query: ListRunsQuery) -> Result<i64, DatabaseError>;
}

pub struct RunServiceImpl {
    db_pool: Arc<DbPool>,
}

impl RunServiceImpl {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }

    fn build_filters(&self, query: &ListRunsQuery) -> String {
        let mut conditions = vec![];

        if let Some(project_id) = &query.project_id {
            let escaped_project = project_id.replace("'", "''");
            conditions.push(format!("project_id = '{}'", escaped_project));
        }

        if let Some(run_ids) = &query.run_ids {
            let values = run_ids
                .iter()
                .map(|id| format!("'{}'", id.replace("'", "''")))
                .collect::<Vec<_>>()
                .join(",");
            conditions.push(format!("run_id IN ({})", values));
        }

        if let Some(thread_ids) = &query.thread_ids {
            let values = thread_ids
                .iter()
                .map(|id| format!("'{}'", id.replace("'", "''")))
                .collect::<Vec<_>>()
                .join(",");
            conditions.push(format!("thread_id IN ({})", values));
        }

        if let Some(trace_ids) = &query.trace_ids {
            let values = trace_ids
                .iter()
                .map(|id| format!("'{}'", id.replace("'", "''")))
                .collect::<Vec<_>>()
                .join(",");
            conditions.push(format!("trace_id IN ({})", values));
        }

        if let Some(model_name) = &query.model_name {
            let escaped_model = model_name.replace("'", "''");
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

impl RunService for RunServiceImpl {
    fn list(&self, query: ListRunsQuery) -> Result<Vec<RunUsageInformation>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let filter_clause = self.build_filters(&query);

        let sql = format!(
            r#"
            SELECT
              run_id,
              COALESCE(json_group_array(DISTINCT thread_id) FILTER (WHERE thread_id IS NOT NULL), '[]') as thread_ids_json,
              COALESCE(json_group_array(DISTINCT trace_id), '[]') as trace_ids_json,
              COALESCE(json_group_array(DISTINCT request_model) FILTER (WHERE request_model IS NOT NULL), '[]') as request_models_json,
              COALESCE(json_group_array(DISTINCT used_model) FILTER (WHERE used_model IS NOT NULL), '[]') as used_models_json,
              CAST(SUM(CASE WHEN operation_name = 'model_call' THEN 1 ELSE 0 END) AS BIGINT) as llm_calls,
              SUM(COALESCE(CAST(json_extract(attribute, '$.cost') as REAL), 0)) as cost,
              CAST(SUM(COALESCE(CAST(json_extract(json_extract(attribute, '$.usage'), '$.input_tokens') AS INTEGER), 0)) AS BIGINT) as input_tokens,
              CAST(SUM(COALESCE(CAST(json_extract(json_extract(attribute, '$.usage'), '$.output_tokens') AS INTEGER), 0)) AS BIGINT) as output_tokens,
              MIN(start_time_us) as start_time_us,
              MAX(finish_time_us) as finish_time_us,
              COALESCE(json_group_array(DISTINCT error_msg) FILTER (WHERE error_msg IS NOT NULL), '[]') as errors_json,
              '[]' as used_tools_json,
              '[]' as mcp_template_definition_ids_json
            FROM (
              SELECT
                run_id,
                thread_id,
                trace_id,
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
              WHERE run_id IS NOT NULL
                {}
            )
            GROUP BY run_id
            ORDER BY start_time_us DESC
            LIMIT {} OFFSET {}
            "#,
            filter_clause, query.limit, query.offset
        );

        let results = diesel::sql_query(sql)
            .load::<RunUsageInformation>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(results)
    }

    fn count(&self, query: ListRunsQuery) -> Result<i64, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let filter_clause = self.build_filters(&query);

        #[derive(QueryableByName)]
        struct CountResult {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            count: i64,
        }

        let sql = format!(
            r#"
            SELECT COUNT(DISTINCT run_id) as count
            FROM traces
            WHERE run_id IS NOT NULL
              {}
            "#,
            filter_clause
        );

        let result = diesel::sql_query(sql)
            .get_result::<CountResult>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(result.count)
    }
}
