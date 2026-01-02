-- Create metrics table for storing OpenTelemetry metrics
CREATE TABLE metrics (
    metric_name TEXT NOT NULL,
    metric_type TEXT NOT NULL, -- counter, histogram, gauge, updowncounter
    value REAL NOT NULL,
    timestamp_us BIGINT NOT NULL,
    attributes TEXT NOT NULL, -- JSON stored as text
    project_id TEXT,
    thread_id TEXT,
    run_id TEXT,
    trace_id TEXT, -- For correlation with traces
    span_id TEXT,  -- For correlation with spans
    PRIMARY KEY (metric_name, timestamp_us, attributes, trace_id, span_id)
);

-- Create indexes for common query patterns
CREATE INDEX idx_metrics_metric_name ON metrics(metric_name);
CREATE INDEX idx_metrics_timestamp_us ON metrics(timestamp_us);
CREATE INDEX idx_metrics_project_id ON metrics(project_id);
CREATE INDEX idx_metrics_thread_id ON metrics(thread_id);
CREATE INDEX idx_metrics_run_id ON metrics(run_id);
CREATE INDEX idx_metrics_trace_id ON metrics(trace_id);
CREATE INDEX idx_metrics_span_id ON metrics(span_id);
-- Composite index for time-range queries with project
CREATE INDEX idx_metrics_project_time ON metrics(project_id, timestamp_us);
-- Composite index for metric name and time range queries
CREATE INDEX idx_metrics_name_time ON metrics(metric_name, timestamp_us);
