-- Drop indexes first
DROP INDEX IF EXISTS idx_metrics_name_time;
DROP INDEX IF EXISTS idx_metrics_project_time;
DROP INDEX IF EXISTS idx_metrics_span_id;
DROP INDEX IF EXISTS idx_metrics_trace_id;
DROP INDEX IF EXISTS idx_metrics_run_id;
DROP INDEX IF EXISTS idx_metrics_thread_id;
DROP INDEX IF EXISTS idx_metrics_project_id;
DROP INDEX IF EXISTS idx_metrics_timestamp_us;
DROP INDEX IF EXISTS idx_metrics_metric_name;

-- Drop the metrics table
DROP TABLE IF EXISTS metrics;
