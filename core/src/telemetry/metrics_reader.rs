use crate::telemetry::metrics_database::SqliteMetricsWriterTransport;
use crate::metadata::pool::DbPool;
use std::sync::Arc;

/// A metric reader that writes metrics to SQLite
/// 
/// NOTE: Due to OpenTelemetry SDK API limitations (ResourceMetrics has private fields),
/// this is currently a placeholder. Internal metrics are exported via OTLP to external
/// backends, and can also be received via MetricsServiceServer if configured to loopback.
/// 
/// Future implementation options:
/// 1. Use a custom MetricExporter wrapper that writes to both OTLP and SQLite
/// 2. Configure internal metrics to also be sent to MetricsServiceServer (loopback)
/// 3. Wait for OpenTelemetry SDK to expose public APIs for ResourceMetrics access
/// 
/// For now, external metrics (from other systems) are fully supported via MetricsServiceServer.
pub struct SqliteMetricReader {
    writer: Arc<SqliteMetricsWriterTransport>,
}

impl SqliteMetricReader {
    pub fn new(db_pool: DbPool) -> Self {
        Self {
            writer: Arc::new(SqliteMetricsWriterTransport::new(db_pool)),
        }
    }

    /// This method is a placeholder for future implementation
    /// 
    /// Current status: Internal metrics are exported via OTLP to external backends.
    /// To also store them in SQLite, you can:
    /// 1. Configure the OTLP endpoint to point to the local MetricsServiceServer (loopback)
    /// 2. Or wait for OpenTelemetry SDK to provide public APIs for accessing ResourceMetrics
    pub fn write_metrics(&self, _resource_metrics: &opentelemetry_sdk::metrics::data::ResourceMetrics) {
        // TODO: Implement when OpenTelemetry SDK provides public APIs
        // The ResourceMetrics structure has private fields that prevent direct access
        tracing::debug!("SqliteMetricReader: Internal metrics are currently exported via OTLP only. To store them in SQLite, configure OTLP to also send to local MetricsServiceServer.");
    }
}
