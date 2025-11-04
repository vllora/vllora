use crate::database::DatabaseTransport;
use crate::metadata::models::trace::DbNewTrace;
use crate::metadata::pool::DbPool;
use crate::metadata::services::trace::TraceServiceImpl;
use crate::telemetry::SpanWriterTransport;
use crate::GatewayError;
use crate::GatewayResult;
use serde_json::Value;
use std::collections::HashMap;

pub struct DatabaseSpanWritter {
    transport: Box<dyn DatabaseTransport + Send + Sync>,
}

impl DatabaseSpanWritter {
    pub fn new(transport: Box<dyn DatabaseTransport + Send + Sync>) -> Self {
        Self { transport }
    }
}

#[async_trait::async_trait]
impl SpanWriterTransport for DatabaseSpanWritter {
    async fn insert_values(
        &self,
        table_name: &str,
        columns: &[&str],
        body: Vec<Vec<Value>>,
    ) -> GatewayResult<String> {
        self.transport
            .insert_values(table_name, columns, body)
            .await
            .map_err(|e| GatewayError::CustomError(e.to_string()))
    }
}

pub struct SqliteTraceWriterTransport {
    trace_service: TraceServiceImpl,
}

impl SqliteTraceWriterTransport {
    pub fn new(db_pool: DbPool) -> Self {
        Self {
            trace_service: TraceServiceImpl::new(db_pool),
        }
    }

    fn convert_row_to_trace(&self, columns: &[Value]) -> Result<DbNewTrace, String> {
        // Row format from SpanWriter.process():
        // [trace_id, span_id, parent_span_id, operation_name, start_time_us, finish_time_us,
        //  finish_date, kind, attribute, tenant_id, project_id, thread_id, tags, run_id]

        if columns.len() < 14 {
            return Err(format!("Expected 14 columns, got {}", columns.len()));
        }

        let trace_id = columns[0]
            .as_str()
            .ok_or("trace_id not a string")?
            .to_string();
        let span_id = columns[1]
            .as_u64()
            .ok_or("span_id not a number")?
            .to_string();

        let parent_span_id = if columns[2].is_null() {
            None
        } else {
            Some(
                columns[2]
                    .as_u64()
                    .ok_or("parent_span_id not a number")?
                    .to_string(),
            )
        };

        let operation_name = columns[3]
            .as_str()
            .ok_or("operation_name not a string")?
            .to_string();
        let start_time_us = columns[4].as_i64().ok_or("start_time_us not a number")?;
        let finish_time_us = columns[5].as_i64().ok_or("finish_time_us not a number")?;
        // columns[6] is finish_date (not used in SQLite schema)
        // columns[7] is kind (not used in SQLite schema)

        let attribute = if columns[8].is_object() {
            columns[8]
                .as_object()
                .ok_or("attribute not an object")?
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<String, Value>>()
        } else {
            HashMap::new()
        };

        // columns[9] is tenant_id (not used in SQLite schema)

        let project_id = if columns[10].is_null() {
            None
        } else {
            Some(
                columns[10]
                    .as_str()
                    .ok_or("project_id not a string")?
                    .to_string(),
            )
        };

        let thread_id = if columns[11].is_null() {
            None
        } else {
            Some(
                columns[11]
                    .as_str()
                    .ok_or("thread_id not a string")?
                    .to_string(),
            )
        };

        // columns[12] is tags (not used in SQLite schema)

        let run_id = if columns[13].is_null() {
            None
        } else {
            Some(
                columns[13]
                    .as_str()
                    .ok_or("run_id not a string")?
                    .to_string(),
            )
        };

        DbNewTrace::new(
            trace_id,
            span_id,
            thread_id,
            parent_span_id,
            operation_name,
            start_time_us,
            finish_time_us,
            attribute,
            run_id,
            project_id,
        )
        .map_err(|e| format!("Failed to create DbNewTrace: {}", e))
    }
}

#[async_trait::async_trait]
impl SpanWriterTransport for SqliteTraceWriterTransport {
    async fn insert_values(
        &self,
        _table_name: &str,
        _columns: &[&str],
        body: Vec<Vec<Value>>,
    ) -> GatewayResult<String> {
        if body.is_empty() {
            return Ok("0".to_string());
        }

        let traces: Result<Vec<DbNewTrace>, String> = body
            .iter()
            .map(|row| self.convert_row_to_trace(row))
            .collect();

        let traces = traces
            .map_err(|e| GatewayError::CustomError(format!("Failed to convert traces: {}", e)))?;

        let inserted_count = self.trace_service.insert_many(traces).map_err(|e| {
            tracing::error!("Failed to insert traces: {}", e);
            GatewayError::CustomError(format!("Failed to insert traces: {}", e))
        })?;

        Ok(inserted_count.to_string())
    }
}
