use crate::metadata::error::DatabaseError;
use crate::metadata::models::trace::{DbTrace, DbNewTrace};
use crate::metadata::pool::DbPool;
use crate::metadata::schema::traces;
use diesel::prelude::*;
use diesel::sql_types::{Text, Nullable};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ListTracesQuery {
    pub project_id: Option<String>,
    pub run_id: Option<String>,
    pub thread_ids: Option<Vec<String>>,
    pub start_time_min: Option<i64>,
    pub start_time_max: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

pub trait TraceService {
    fn list(&self, query: ListTracesQuery) -> Result<Vec<DbTrace>, DatabaseError>;
    fn get_by_run_id(&self, run_id: &str, project_id: Option<&str>, limit: i64, offset: i64) -> Result<Vec<DbTrace>, DatabaseError>;
    fn count(&self, query: ListTracesQuery) -> Result<i64, DatabaseError>;
    fn get_child_attributes(&self, trace_ids: &[String], span_ids: &[String], project_id: Option<&str>) -> Result<HashMap<String, Option<String>>, DatabaseError>;
}

pub struct TraceServiceImpl {
    db_pool: Arc<DbPool>,
}

impl TraceServiceImpl {
    pub fn new(db_pool: Arc<DbPool>) -> Self {
        Self { db_pool }
    }
}

impl TraceService for TraceServiceImpl {
    fn list(&self, query: ListTracesQuery) -> Result<Vec<DbTrace>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut db_query = traces::table.into_boxed();

        // Apply project_id filter
        if let Some(project_id) = &query.project_id {
            db_query = db_query.filter(traces::project_id.eq(project_id));
        }

        // Apply run_id filter
        if let Some(run_id) = &query.run_id {
            db_query = db_query.filter(traces::run_id.eq(run_id));
        }

        // Apply thread_ids filter
        if let Some(thread_ids) = &query.thread_ids {
            if !thread_ids.is_empty() {
                db_query = db_query.filter(traces::thread_id.eq_any(thread_ids));
            }
        }

        // Apply time range filters
        if let Some(start_min) = query.start_time_min {
            db_query = db_query.filter(traces::start_time_us.ge(start_min));
        }

        if let Some(start_max) = query.start_time_max {
            db_query = db_query.filter(traces::start_time_us.le(start_max));
        }

        // Order by start_time_us descending, apply limit and offset
        let results = db_query
            .order(traces::start_time_us.desc())
            .limit(query.limit)
            .offset(query.offset)
            .load::<DbTrace>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(results)
    }

    fn get_by_run_id(&self, run_id: &str, project_id: Option<&str>, limit: i64, offset: i64) -> Result<Vec<DbTrace>, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut query = traces::table
            .filter(traces::run_id.eq(run_id))
            .into_boxed();

        // Apply project_id filter if provided
        if let Some(project_id) = project_id {
            query = query.filter(traces::project_id.eq(project_id));
        }

        let results = query
            .order(traces::start_time_us.asc())
            .limit(limit)
            .offset(offset)
            .load::<DbTrace>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(results)
    }

    fn count(&self, query: ListTracesQuery) -> Result<i64, DatabaseError> {
        let mut conn = self.db_pool.get()?;

        let mut db_query = traces::table.into_boxed();

        // Apply project_id filter
        if let Some(project_id) = &query.project_id {
            db_query = db_query.filter(traces::project_id.eq(project_id));
        }

        // Apply run_id filter
        if let Some(run_id) = &query.run_id {
            db_query = db_query.filter(traces::run_id.eq(run_id));
        }

        // Apply thread_ids filter
        if let Some(thread_ids) = &query.thread_ids {
            if !thread_ids.is_empty() {
                db_query = db_query.filter(traces::thread_id.eq_any(thread_ids));
            }
        }

        // Apply time range filters
        if let Some(start_min) = query.start_time_min {
            db_query = db_query.filter(traces::start_time_us.ge(start_min));
        }

        if let Some(start_max) = query.start_time_max {
            db_query = db_query.filter(traces::start_time_us.le(start_max));
        }

        let count = db_query
            .count()
            .get_result::<i64>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        Ok(count)
    }

    fn get_child_attributes(&self, trace_ids: &[String], span_ids: &[String], project_id: Option<&str>) -> Result<HashMap<String, Option<String>>, DatabaseError> {
        if trace_ids.is_empty() || span_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.db_pool.get()?;

        // Build the query to find model_call children for each parent span
        // Using a subquery to get the first model_call child for each parent
        #[derive(QueryableByName)]
        struct ChildAttr {
            #[diesel(sql_type = Text)]
            parent_span_id: String,
            #[diesel(sql_type = Nullable<Text>)]
            child_attribute: Option<String>,
        }

        let trace_ids_str = trace_ids.iter()
            .map(|id| format!("'{}'", id))
            .collect::<Vec<_>>()
            .join(",");

        let span_ids_str = span_ids.iter()
            .map(|id| format!("'{}'", id))
            .collect::<Vec<_>>()
            .join(",");

        let project_filter = if let Some(pid) = project_id {
            format!(" AND project_id = '{}'", pid)
        } else {
            String::new()
        };

        let query = format!(
            "SELECT parent_span_id, attribute as child_attribute
             FROM (
                 SELECT parent_span_id, attribute,
                        ROW_NUMBER() OVER (PARTITION BY parent_span_id ORDER BY start_time_us) as rn
                 FROM traces
                 WHERE trace_id IN ({})
                   AND parent_span_id IN ({})
                   AND operation_name = 'model_call'{}
             )
             WHERE rn = 1",
            trace_ids_str, span_ids_str, project_filter
        );

        let results = diesel::sql_query(query)
            .load::<ChildAttr>(&mut conn)
            .map_err(DatabaseError::QueryError)?;

        let mut map = HashMap::new();
        for result in results {
            map.insert(result.parent_span_id, result.child_attribute);
        }

        Ok(map)
    }
}

impl TraceServiceImpl {
    pub fn insert_many(&self, trace_list: Vec<DbNewTrace>) -> Result<usize, DatabaseError> {
        if trace_list.is_empty() {
            return Ok(0);
        }

        let mut conn = self.db_pool.get()?;
        let mut inserted_count = 0;

        for trace in &trace_list {
            diesel::insert_into(traces::table)
                .values(trace)
                .execute(&mut conn)?;
            inserted_count += 1;
        }

        Ok(inserted_count)
    }
}
