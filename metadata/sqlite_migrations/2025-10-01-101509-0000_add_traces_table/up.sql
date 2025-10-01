-- Your SQL goes here
CREATE TABLE traces (
    trace_id TEXT NOT NULL,
    span_id TEXT NOT NULL,
    thread_id TEXT,
    parent_span_id TEXT,
    operation_name TEXT NOT NULL,
    start_time_us INTEGER NOT NULL,
    finish_time_us INTEGER NOT NULL,
    attribute TEXT NOT NULL, -- JSON stored as text
    run_id TEXT,
    project_id TEXT,
    PRIMARY KEY (trace_id, span_id)
);

-- Create indexes for common query patterns
CREATE INDEX idx_traces_trace_id ON traces(trace_id);
CREATE INDEX idx_traces_thread_id ON traces(thread_id);
CREATE INDEX idx_traces_run_id ON traces(run_id);
CREATE INDEX idx_traces_project_id ON traces(project_id);
CREATE INDEX idx_traces_start_time_us ON traces(start_time_us);
CREATE INDEX idx_traces_finish_time_us ON traces(finish_time_us);
CREATE INDEX idx_traces_parent_span_id ON traces(parent_span_id);

-- Composite index for child_attribute JOIN query (trace_id, parent_span_id, operation_name, start_time_us)
CREATE INDEX idx_traces_child_lookup ON traces(trace_id, parent_span_id, operation_name, start_time_us);
