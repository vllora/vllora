use crate::database::DatabaseTransport;
use crate::otel::SpanWriterTransport;
use crate::GatewayError;
use crate::GatewayResult;
use serde_json::Value;

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
