use std::sync::Arc;
use vllora_core::metadata::models::metric::DbNewMetric;
use vllora_core::telemetry::metrics_database::SqliteMetricsWriterTransport;
use vllora_telemetry::MetricsDataPoint;
use vllora_telemetry::MetricsWriterTransport;

/// Adapter that implements MetricsWriterTransport using SqliteMetricsWriterTransport
pub struct SqliteMetricsWriterAdapter {
    writer: Arc<SqliteMetricsWriterTransport>,
}

impl SqliteMetricsWriterAdapter {
    pub fn new(writer: Arc<SqliteMetricsWriterTransport>) -> Self {
        Self { writer }
    }
}

#[async_trait::async_trait]
impl MetricsWriterTransport for SqliteMetricsWriterAdapter {
    async fn write_metrics(
        &self,
        metrics: Vec<MetricsDataPoint>,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let db_metrics: Result<Vec<DbNewMetric>, serde_json::Error> = metrics
            .into_iter()
            .map(|m| {
                DbNewMetric::new(
                    m.metric_name,
                    m.metric_type,
                    m.value,
                    m.timestamp_us,
                    m.attributes,
                    m.project_id,
                    m.thread_id,
                    m.run_id,
                    m.trace_id,
                    m.span_id,
                )
            })
            .collect();

        match db_metrics {
            Ok(db_metrics) => self.writer.write_metrics(db_metrics).map_err(|e| {
                Box::new(std::io::Error::other(e)) as Box<dyn std::error::Error + Send + Sync>
            }),
            Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        }
    }
}
