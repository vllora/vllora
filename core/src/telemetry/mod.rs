pub mod database;
pub mod events;

use crate::types::GatewayTenant;
use std::collections::HashMap;
use std::time::Duration;
use std::{future::Ready, sync::Arc};

use crate::GatewayResult;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::header::HeaderMap,
    HttpMessage,
};
use dashmap::DashMap;
use futures::future::LocalBoxFuture;
use opentelemetry::{
    baggage::BaggageExt, propagation::TextMapPropagator, trace::FutureExt, Context, KeyValue,
};
use opentelemetry::{
    propagation::Extractor,
    trace::{SpanId, SpanKind, TraceId},
};
pub use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::TraceServiceServer;
use opentelemetry_proto::tonic::{
    collector::trace::v1::{
        trace_service_server::TraceService, ExportTracePartialSuccess, ExportTraceServiceRequest,
        ExportTraceServiceResponse,
    },
    common::v1::{self as otel_proto, any_value, AnyValue, KeyValueList},
    trace::v1::span as otel_span,
};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SpanData;
use serde::{Serialize, Serializer};
use serde_json::Value;
use tokio::select;
use tokio::sync::{broadcast, mpsc};
use tonic::metadata::MetadataMap;
use uuid::Uuid;

pub fn trace_id_uuid(trace_id: TraceId) -> Uuid {
    Uuid::from_bytes(trace_id.to_bytes())
}

#[derive(Clone)]
pub struct AdditionalContext(pub HashMap<String, String>);

impl AdditionalContext {
    pub fn new(context: HashMap<String, String>) -> Self {
        Self(context)
    }
}

#[async_trait::async_trait]
pub trait TraceTenantResolver: Send + Sync + std::fmt::Debug {
    async fn get_tenant_id(&self, metadata: &MetadataMap) -> Option<(String, String)>;
}

#[async_trait::async_trait]
pub trait SpanWriterTransport: Send + Sync {
    async fn insert_values(
        &self,
        table_name: &str,
        columns: &[&str],
        body: Vec<Vec<Value>>,
    ) -> GatewayResult<String>;
}

/// A no-op implementation of SpanWriterTransport that discards all data
pub struct NoOpSpanWriter;

impl Default for NoOpSpanWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl NoOpSpanWriter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl SpanWriterTransport for NoOpSpanWriter {
    async fn insert_values(
        &self,
        _table_name: &str,
        _columns: &[&str],
        _body: Vec<Vec<Value>>,
    ) -> GatewayResult<String> {
        Ok("no-op".to_string())
    }
}

pub(crate) struct SpanWriter {
    pub(crate) transport: Box<dyn SpanWriterTransport>,
    pub(crate) receiver: tokio::sync::mpsc::Receiver<Span>,
    pub(crate) buf: Vec<Vec<Value>>,
    pub(crate) finished_traces: Vec<TraceId>,
}

impl SpanWriter {
    pub(crate) fn process(&mut self, span: Span) {
        let Span {
            trace_id,
            span_id,
            parent_span_id,
            operation_name,
            start_time_unix_nano,
            end_time_unix_nano,
            kind: span_kind,
            attributes,
            tenant_id,
            project_id,
            thread_id,
            tags,
            run_id,
        } = span;
        if parent_span_id.is_none() {
            self.finished_traces.push(trace_id);
        }
        self.buf.push(vec![
            trace_id_uuid(trace_id).to_string().into(),
            u64::from_be_bytes(span_id.to_bytes()).into(),
            parent_span_id.map_or(Value::Null, |span_id| {
                u64::from_be_bytes(span_id.to_bytes()).into()
            }),
            operation_name.into(),
            (start_time_unix_nano / 1000).into(),
            (end_time_unix_nano / 1000).into(),
            serde_json::to_value(
                chrono::DateTime::from_timestamp_nanos(end_time_unix_nano.try_into().unwrap())
                    .date_naive(),
            )
            .unwrap(),
            match span_kind {
                SpanKind::Client => "CLIENT",
                SpanKind::Server => "SERVER",
                SpanKind::Producer => "PRODUCER",
                SpanKind::Consumer => "CONSUMER",
                SpanKind::Internal => "INTERNAL",
            }
            .into(),
            attributes.into(),
            tenant_id.into(),
            project_id.into(),
            thread_id.into(),
            tags.into(),
            run_id.into(),
        ]);
    }

    pub(crate) async fn flush(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let result = self
            .transport
            .insert_values(
                "vllora.traces",
                &[
                    "trace_id",
                    "span_id",
                    "parent_span_id",
                    "operation_name",
                    "start_time_us",
                    "finish_time_us",
                    "finish_date",
                    "kind",
                    "attribute",
                    "tenant_id",
                    "project_id",
                    "thread_id",
                    "tags",
                    "run_id",
                ],
                self.buf.clone(),
            )
            .await;
        if let Err(e) = result {
            tracing::error!("{e}");
        }
        self.buf.clear();
    }

    pub(crate) async fn run(mut self) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            select! {
                span = self.receiver.recv() => {
                    let Some(span) = span else {
                        break;
                    };
                    self.process(span);
                    if self.buf.len() > 1000 {
                        self.flush().await
                    }
                }
                _ = interval.tick() => {
                    self.flush().await
                }
            }
        }
        while let Some(span) = self.receiver.recv().await {
            self.process(span);
        }
    }
}

#[derive(Debug)]
pub struct TraceServiceImpl {
    pub(crate) writer_sender: mpsc::Sender<Span>,
    pub(crate) tenant_resolver: Box<dyn TraceTenantResolver>,
    pub(crate) project_trace_senders: Arc<ProjectTraceMap>,
}

impl TraceServiceImpl {
    pub fn new(
        transport: Box<dyn SpanWriterTransport>,
        tenant_resolver: Box<dyn TraceTenantResolver>,
        project_trace_senders: Arc<ProjectTraceMap>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1000);
        let writer = SpanWriter {
            transport,
            receiver,
            finished_traces: Default::default(),
            buf: Default::default(),
        };
        tokio::spawn(writer.run());
        Self {
            writer_sender: sender,
            tenant_resolver,
            project_trace_senders,
        }
    }
}

pub(crate) fn serialize_any_value(value: AnyValue) -> serde_json::Value {
    let Some(value) = value.value else {
        return serde_json::Value::Null;
    };
    match value {
        any_value::Value::StringValue(string) => string.into(),
        any_value::Value::BoolValue(bool) => bool.into(),
        any_value::Value::IntValue(int) => int.into(),
        any_value::Value::DoubleValue(double) => double.into(),
        any_value::Value::ArrayValue(otel_proto::ArrayValue { values }) => {
            values.into_iter().map(serialize_any_value).collect()
        }
        any_value::Value::KvlistValue(KeyValueList { values }) => values
            .into_iter()
            .map(|otel_proto::KeyValue { key, value }| {
                (
                    key.to_string(),
                    value.map_or(Value::Null, serialize_any_value),
                )
            })
            .collect::<serde_json::Map<_, _>>()
            .into(),
        any_value::Value::BytesValue(bytes) => bytes.into(),
    }
}

const DEFAULT_SPAN_ID: SpanId = SpanId::from_bytes([0; 8]);

pub fn map_span(span: SpanData) -> Option<Span> {
    let kind = match span.span_kind {
        opentelemetry::trace::SpanKind::Internal => SpanKind::Internal,
        opentelemetry::trace::SpanKind::Server => SpanKind::Server,
        opentelemetry::trace::SpanKind::Client => SpanKind::Client,
        opentelemetry::trace::SpanKind::Producer => SpanKind::Producer,
        opentelemetry::trace::SpanKind::Consumer => SpanKind::Consumer,
    };

    let trace_id = span.span_context.trace_id();
    let span_id = span.span_context.span_id();
    let parent_span_id = match span.parent_span_id {
        parent_span_id if parent_span_id != DEFAULT_SPAN_ID => {
            Some(SpanId::from_bytes(parent_span_id.to_bytes()))
        }
        _ => None,
    };

    let mut attributes: serde_json::Map<String, Value> = span
        .attributes
        .into_iter()
        .map(|attr| (attr.key.to_string(), serialize_any_value(attr.value.into())))
        .collect();

    let tenant_id = attributes
        .remove("vllora.tenant")
        .and_then(|v| Some(v.as_str()?.to_owned()));

    let project_id = attributes
        .remove("vllora.project_id")
        .and_then(|v| Some(v.as_str()?.to_owned()));

    let thread_id = attributes
        .remove("vllora.thread_id")
        .and_then(|v| Some(v.as_str()?.to_owned()));

    let run_id = attributes
        .remove("vllora.run_id")
        .and_then(|v| Some(v.as_str()?.to_owned()));

    let label = attributes
        .remove("vllora.label")
        .and_then(|v| Some(v.as_str()?.to_owned()));

    if !attributes.contains_key("label") {
        if let Some(label) = label {
            attributes.insert("label".to_string(), label.into());
        }
    }

    let tags_value = attributes.remove("tags");
    let mut tags: serde_json::Map<String, Value> = Default::default();
    if let Some(Value::String(s)) = tags_value {
        tags = serde_json::from_str(&s).ok().unwrap_or_default();
    }

    // Convert SystemTime to unix nanoseconds
    let start_time_unix_nano = span
        .start_time
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as u64;
    let end_time_unix_nano = span
        .end_time
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as u64;

    let span = Span {
        trace_id,
        span_id,
        parent_span_id,
        operation_name: span.name.to_string(),
        start_time_unix_nano,
        end_time_unix_nano,
        kind,
        attributes,
        tenant_id,
        project_id: project_id.clone(),
        thread_id,
        tags,
        run_id,
    };

    Some(span)
}

#[tonic::async_trait]
impl TraceService for TraceServiceImpl {
    #[tracing::instrument(level = "info")]
    async fn export(
        &self,
        request: tonic::Request<ExportTraceServiceRequest>,
    ) -> tonic::Result<tonic::Response<ExportTraceServiceResponse>> {
        let mut rejected = 0;
        macro_rules! try_ {
            ($v:expr) => {
                if let Ok(v) = $v {
                    v
                } else {
                    rejected += 1;
                    continue;
                }
            };
        }

        let headers = request.metadata();
        let tenant_from_header = self.tenant_resolver.get_tenant_id(headers).await;

        for resource in request.into_inner().resource_spans {
            for scope in resource.scope_spans {
                for span in scope.spans {
                    let kind = match span.kind() {
                        otel_span::SpanKind::Unspecified => SpanKind::Internal,
                        otel_span::SpanKind::Internal => SpanKind::Internal,
                        otel_span::SpanKind::Server => SpanKind::Server,
                        otel_span::SpanKind::Client => SpanKind::Client,
                        otel_span::SpanKind::Producer => SpanKind::Producer,
                        otel_span::SpanKind::Consumer => SpanKind::Consumer,
                    };

                    let trace_id = TraceId::from_bytes(try_!(span.trace_id.try_into()));
                    let span_id = SpanId::from_bytes(try_!(span.span_id.try_into()));
                    let parent_span_id = if span.parent_span_id.is_empty() {
                        None
                    } else {
                        Some(SpanId::from_bytes(try_!(span.parent_span_id.try_into())))
                    };

                    let mut attributes: serde_json::Map<String, Value> = span
                        .attributes
                        .into_iter()
                        .map(|attr| {
                            (
                                attr.key,
                                attr.value.map_or(Value::Null, serialize_any_value),
                            )
                        })
                        .collect();

                    let tenant_id = attributes
                        .remove("vllora.tenant")
                        .and_then(|v| Some(v.as_str()?.to_owned()))
                        .or(tenant_from_header.as_ref().map(|v| v.0.clone()));

                    if tenant_id.is_none() {
                        tracing::debug!(
                            target: "otel",
                            "No tenant id found in span {} with attributes: {:#?}",
                            span.name,
                            attributes
                        );
                        continue;
                    }

                    let project_id = attributes
                        .remove("vllora.project_id")
                        .and_then(|v| Some(v.as_str()?.to_owned()))
                        .or(tenant_from_header.as_ref().map(|v| v.1.clone()));

                    let thread_id = attributes
                        .remove("vllora.thread_id")
                        .and_then(|v| Some(v.as_str()?.to_owned()));

                    let run_id = attributes
                        .remove("vllora.run_id")
                        .and_then(|v| Some(v.as_str()?.to_owned()));

                    let label = attributes
                        .remove("vllora.label")
                        .and_then(|v| Some(v.as_str()?.to_owned()));

                    if !attributes.contains_key("label") {
                        if let Some(label) = label {
                            attributes.insert("label".to_string(), label.into());
                        }
                    }

                    let tags_value = attributes.remove("tags");
                    let mut tags: serde_json::Map<String, Value> = Default::default();
                    if let Some(Value::String(s)) = tags_value {
                        tags = serde_json::from_str(&s).ok().unwrap_or_default();
                    }

                    let is_client = attributes.get("vllora.client_name").is_some();

                    let span = Span {
                        trace_id,
                        span_id,
                        parent_span_id,
                        operation_name: span.name,
                        start_time_unix_nano: span.start_time_unix_nano,
                        end_time_unix_nano: span.end_time_unix_nano,
                        kind,
                        attributes,
                        tenant_id,
                        project_id: project_id.clone(),
                        thread_id,
                        tags,
                        run_id,
                    };

                    if is_client {
                        if let Some(project_id) = project_id.as_ref() {
                            if let Some(sender) =
                                self.project_trace_senders.get(project_id).as_deref()
                            {
                                let _result = sender.send(span.clone());
                            }
                        }
                    }

                    self.writer_sender.send(span).await.unwrap();
                }
            }
        }
        Ok(tonic::Response::new(ExportTraceServiceResponse {
            partial_success: Some(ExportTracePartialSuccess {
                rejected_spans: rejected,
                error_message: "".into(),
            }),
        }))
    }
}

#[derive(Clone, Serialize)]
pub struct Span {
    #[serde(serialize_with = "serialize_trace_id")]
    pub trace_id: TraceId,
    #[serde(serialize_with = "serialize_span_id")]
    pub span_id: SpanId,
    #[serde(serialize_with = "serialize_option_span_id")]
    pub parent_span_id: Option<SpanId>,
    pub operation_name: String,
    #[serde(skip_serializing)]
    pub kind: SpanKind,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    #[serde(rename = "attribute")]
    pub attributes: serde_json::Map<String, serde_json::Value>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub thread_id: Option<String>,
    pub tags: serde_json::Map<String, serde_json::Value>,
    pub run_id: Option<String>,
}

fn serialize_span_id<S>(span_id: &SpanId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&span_id.to_string())
}

fn serialize_option_span_id<S>(span_id: &Option<SpanId>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match span_id {
        Some(span_id) => serializer.serialize_str(&span_id.to_string()),
        None => serializer.serialize_none(),
    }
}

fn serialize_trace_id<S>(trace_id: &TraceId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&trace_id.to_string())
}

pub struct TracingContext;
impl<S, B> Transform<S, ServiceRequest> for TracingContext
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = TracingContextMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(TracingContextMiddleware { service }))
    }
}

pub struct TracingContextMiddleware<S> {
    service: S,
}

pub type ProjectTraceMap = DashMap<String, broadcast::Sender<Span>>;

impl<S, B> Service<ServiceRequest> for TracingContextMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let propagator = TraceContextPropagator::new();
        let context =
            propagator.extract_with_context(&Context::new(), &HeaderExtractor(req.headers()));
        let tenant = req.extensions().get::<GatewayTenant>().cloned();

        match tenant {
            Some(tenant) => {
                let tenant_name = tenant.name.clone();
                let project_slug = tenant.project_slug.clone();

                let mut key_values = vec![
                    KeyValue::new("vllora.tenant", tenant_name),
                    KeyValue::new("vllora.project_id", project_slug),
                ];

                let run_id = req
                    .headers()
                    .get("x-run-id")
                    .and_then(|v| v.to_str().ok().map(|v| v.to_string()));

                if let Some(run_id) = run_id.as_ref() {
                    key_values.push(KeyValue::new("vllora.run_id", run_id.clone()));
                } else {
                    key_values.push(KeyValue::new("vllora.run_id", Uuid::new_v4().to_string()));
                }

                let label = req
                    .headers()
                    .get("x-label")
                    .and_then(|v| v.to_str().ok().map(|v| v.to_string()));

                if let Some(label) = label.as_ref() {
                    key_values.push(KeyValue::new("vllora.label", label.clone()));
                }

                let additional_context = req.extensions().get::<AdditionalContext>().cloned();
                if let Some(additional_context) = additional_context.as_ref() {
                    for (key, value) in additional_context.0.iter() {
                        key_values.push(KeyValue::new(key.clone(), value.clone()));
                    }
                }

                let context = context.with_baggage(key_values);

                let fut = self.service.call(req).with_context(context);
                Box::pin(fut)
            }
            None => {
                tracing::warn!("tenant not found");
                let fut = self.service.call(req);
                Box::pin(fut)
            }
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
