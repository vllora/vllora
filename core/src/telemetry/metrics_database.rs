use crate::metadata::models::metric::DbNewMetric;
use crate::metadata::pool::DbPool;
use crate::metadata::services::metric::MetricsServiceImpl;
use crate::metadata::DatabaseServiceTrait;
use std::collections::HashMap;

pub struct SqliteMetricsWriterTransport {
    metrics_service: MetricsServiceImpl,
}

impl SqliteMetricsWriterTransport {
    pub fn new(db_pool: DbPool) -> Self {
        Self {
            metrics_service: <MetricsServiceImpl as DatabaseServiceTrait>::init(db_pool),
        }
    }

    /// Write metrics to the database
    /// This function will be called by a MetricReader to export metrics
    pub fn write_metrics(&self, metrics: Vec<DbNewMetric>) -> Result<usize, String> {
        if metrics.is_empty() {
            return Ok(0);
        }

        self.metrics_service
            .insert_many(metrics)
            .map_err(|e| format!("Failed to insert metrics: {}", e))
    }

    /// Helper function to create a DbNewMetric from metric data
    pub fn create_metric(
        metric_name: String,
        metric_type: String,
        value: f64,
        timestamp_us: i64,
        attributes: HashMap<String, serde_json::Value>,
        project_id: Option<String>,
        thread_id: Option<String>,
        run_id: Option<String>,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<DbNewMetric, serde_json::Error> {
        DbNewMetric::new(
            metric_name,
            metric_type,
            value,
            timestamp_us,
            attributes,
            project_id,
            thread_id,
            run_id,
            trace_id,
            span_id,
        )
    }
}

impl std::fmt::Debug for SqliteMetricsWriterTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteMetricsWriterTransport").finish()
    }
}
