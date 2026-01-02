pub mod database;
pub mod metrics_database;

use crate::metadata::models::trace::DbTrace;
use std::convert::TryFrom;
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::http::header::HeaderMap;
use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use opentelemetry::propagation::Extractor;
use opentelemetry_sdk::trace::SpanData;
use vllora_telemetry::{map_span, trace_id_uuid, ProjectTraceMap, Span};

pub fn span_to_db_trace(span: &Span) -> Option<DbTrace> {
    let run_id = span.run_id.clone()?;
    let attributes =
        serde_json::to_string(&serde_json::Value::Object(span.attributes.clone())).ok()?;
    let start_time_us = i64::try_from(span.start_time_unix_nano / 1_000).ok()?;
    let finish_time_us = i64::try_from(span.end_time_unix_nano / 1_000).ok()?;

    Some(DbTrace {
        trace_id: trace_id_uuid(span.trace_id).to_string(),
        span_id: u64::from_be_bytes(span.span_id.to_bytes()).to_string(),
        thread_id: span.thread_id.clone(),
        parent_span_id: span
            .parent_span_id
            .as_ref()
            .map(|parent| u64::from_be_bytes(parent.to_bytes()).to_string()),
        operation_name: span.operation_name.clone(),
        start_time_us,
        finish_time_us,
        attribute: attributes,
        run_id: Some(run_id),
        project_id: span.project_id.clone(),
    })
}

#[derive(Clone)]
struct RunSpanBufferEntry {
    spans: Vec<Span>,
    expires_at: Instant,
}

pub struct RunSpanBuffer {
    ttl: Duration,
    inner: DashMap<String, RunSpanBufferEntry>,
}

impl RunSpanBuffer {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            inner: DashMap::new(),
        }
    }

    pub fn insert(&self, span: Span) {
        let Some(run_id) = span.run_id.clone() else {
            return;
        };

        let now = Instant::now();
        let expires_at = now + self.ttl;

        match self.inner.entry(run_id) {
            Entry::Occupied(mut occupied) => {
                let entry = occupied.get_mut();
                if entry.expires_at <= now {
                    entry.spans.clear();
                }
                entry.spans.push(span);
                entry.expires_at = expires_at;
            }
            Entry::Vacant(vacant) => {
                vacant.insert(RunSpanBufferEntry {
                    spans: vec![span],
                    expires_at,
                });
            }
        }
    }

    pub fn get(&self, run_id: &str, project_id: Option<&str>) -> Option<Vec<DbTrace>> {
        let now = Instant::now();
        let entry = self.inner.get(run_id)?;

        if entry.expires_at <= now {
            drop(entry);
            self.inner.remove(run_id);
            return None;
        }

        let mut spans: Vec<DbTrace> = entry
            .spans
            .iter()
            .filter(|span| match (project_id, span.project_id.as_deref()) {
                (Some(expected), Some(actual)) => expected == actual,
                (Some(_), None) => false,
                _ => true,
            })
            .filter_map(span_to_db_trace)
            .collect();
        drop(entry);

        if spans.is_empty() {
            None
        } else {
            spans.sort_by_key(|span| span.start_time_us);
            Some(spans)
        }
    }
}

pub struct HeaderExtractor<'a>(pub &'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|header| header.as_str()).collect()
    }
}

pub struct ProjectTraceSpanExporter {
    project_trace_senders: Arc<ProjectTraceMap>,
}

impl ProjectTraceSpanExporter {
    pub fn new(project_trace_senders: Arc<ProjectTraceMap>) -> Self {
        Self {
            project_trace_senders,
        }
    }
}

impl std::fmt::Debug for ProjectTraceSpanExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectTraceSpanExporter").finish()
    }
}

impl opentelemetry_sdk::trace::SpanExporter for ProjectTraceSpanExporter {
    async fn export(&self, batch: Vec<SpanData>) -> opentelemetry_sdk::error::OTelSdkResult {
        for span in batch {
            if let Some(span) = map_span(span) {
                if let Some(project_id) = span.project_id.as_ref() {
                    if let Some(sender) = self.project_trace_senders.get(project_id).as_deref() {
                        let _result = sender.send(span.clone());
                    }
                }
            }
        }
        opentelemetry_sdk::error::OTelSdkResult::Ok(())
    }
}

pub struct RunSpanBufferExporter {
    buffer: Arc<RunSpanBuffer>,
}

impl RunSpanBufferExporter {
    pub fn new(buffer: Arc<RunSpanBuffer>) -> Self {
        Self { buffer }
    }
}

impl std::fmt::Debug for RunSpanBufferExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunSpanBufferExporter").finish()
    }
}

impl opentelemetry_sdk::trace::SpanExporter for RunSpanBufferExporter {
    async fn export(&self, batch: Vec<SpanData>) -> opentelemetry_sdk::error::OTelSdkResult {
        for span in batch {
            if let Some(span) = map_span(span) {
                if span.run_id.is_some() {
                    self.buffer.insert(span);
                }
            }
        }
        opentelemetry_sdk::error::OTelSdkResult::Ok(())
    }
}
