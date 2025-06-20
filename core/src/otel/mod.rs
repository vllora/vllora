#[cfg(feature = "database")]
pub mod database;

use crate::types::GatewayTenant;
use std::collections::HashMap;
use std::str::FromStr;
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

#[derive(Debug)]
pub struct DummyTraceTenantResolver;

#[async_trait::async_trait]
impl TraceTenantResolver for DummyTraceTenantResolver {
    async fn get_tenant_id(&self, _metadata: &MetadataMap) -> Option<(String, String)> {
        None
    }
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

pub(crate) struct SpanWriter {
    pub(crate) transport: Box<dyn SpanWriterTransport>,
    pub(crate) receiver: tokio::sync::mpsc::Receiver<Span>,
    pub(crate) buf: Vec<Vec<Value>>,
    pub(crate) trace_senders: Arc<TraceMap>,
    pub(crate) finished_traces: Vec<TraceId>,
}

impl SpanWriter {
    pub(crate) fn process(&mut self, span: Span) {
        let Span {
            trace_id,
            parent_trace_id,
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
            parent_trace_id.map_or(Value::Null, |trace_id| {
                trace_id_uuid(trace_id).to_string().into()
            }),
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
                "langdb.traces",
                &[
                    "trace_id",
                    "parent_trace_id",
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
        // Once we've written the full trace, we can safely drop the sender
        for trace_id in self.finished_traces.drain(..) {
            self.trace_senders.remove(&trace_id);
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
    pub(crate) listener_senders: Arc<TraceMap>,
    pub(crate) writer_sender: mpsc::Sender<Span>,
    pub(crate) tenant_resolver: Box<dyn TraceTenantResolver>,
}

impl TraceServiceImpl {
    pub fn new(
        listener_senders: Arc<TraceMap>,
        transport: Box<dyn SpanWriterTransport>,
        tenant_resolver: Box<dyn TraceTenantResolver>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1000);
        let writer = SpanWriter {
            trace_senders: Arc::clone(&listener_senders),
            transport,
            receiver,
            finished_traces: Default::default(),
            buf: Default::default(),
        };
        tokio::spawn(writer.run());
        Self {
            listener_senders,
            writer_sender: sender,
            tenant_resolver,
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

                    tracing::debug!(target: "otel",
                        "Span name {}({}) <- {:?}",
                        span.name,
                        span_id,
                        parent_span_id,
                    );

                    let message_ids = span
                        .attributes
                        .iter()
                        .filter(|attr| attr.key == "message_id")
                        .filter_map(|attr| {
                            attr.value.as_ref().and_then(|v| match &v.value {
                                Some(any_value::Value::StringValue(s)) => Some(s.to_owned()),
                                _ => None,
                            })
                        })
                        .collect::<Vec<String>>();

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
                    
                    if !message_ids.is_empty() {           
                        attributes.insert(
                            "message_id".to_string(),
                            Value::Array(message_ids.iter().map(|s| s.clone().into()).collect()),
                        );
                    }
                    let tenant_id = attributes
                        .remove("langdb.tenant")
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
                        .remove("langdb.project_id")
                        .and_then(|v| Some(v.as_str()?.to_owned()))
                        .or(tenant_from_header.as_ref().map(|v| v.1.clone()));
                    let thread_id = attributes
                        .remove("langdb.thread_id")
                        .and_then(|v| Some(v.as_str()?.to_owned()));
                    let parent_trace_id =
                        attributes.remove("langdb.parent_trace_id").and_then(|v| {
                            let u = Uuid::from_str(v.as_str()?).ok()?;
                            let b = u.into_bytes();
                            Some(TraceId::from_bytes(b))
                        });
                    let run_id = attributes
                        .remove("langdb.run_id")
                        .and_then(|v| Some(v.as_str()?.to_owned()));
                    let langdb_trace_id = attributes
                        .remove("langdb.trace_id")
                        .and_then(|v| Some(v.as_str()?.to_owned()));

                    let label = attributes
                        .remove("langdb.label")
                        .and_then(|v| Some(v.as_str()?.to_owned()));

                    if !attributes.contains_key("label") {
                        if let Some(label) = label {
                            attributes.insert("label".to_string(), label.into());
                        }
                    }

                    let trace_id = match langdb_trace_id {
                        Some(langdb_trace_id) => match Uuid::from_str(&langdb_trace_id) {
                            Ok(u) => TraceId::from_bytes(u.into_bytes()),
                            Err(_) => trace_id,
                        },
                        None => trace_id,
                    };

                    let tags_value = attributes.remove("tags");
                    let mut tags: serde_json::Map<String, Value> = Default::default();
                    if let Some(Value::String(s)) = tags_value {
                        tags = serde_json::from_str(&s).ok().unwrap_or_default();
                    }

                    let span = Span {
                        trace_id,
                        parent_trace_id,
                        span_id,
                        parent_span_id,
                        operation_name: span.name,
                        start_time_unix_nano: span.start_time_unix_nano,
                        end_time_unix_nano: span.end_time_unix_nano,
                        kind,
                        attributes,
                        tenant_id,
                        project_id,
                        thread_id,
                        tags,
                        run_id,
                    };
                    if let Some((sender, _)) = self.listener_senders.get(&trace_id).as_deref() {
                        let _ = sender.send(span.clone());
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

#[derive(Clone)]
pub struct Span {
    pub trace_id: TraceId,
    pub parent_trace_id: Option<TraceId>,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub operation_name: String,
    pub kind: SpanKind,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub attributes: serde_json::Map<String, serde_json::Value>,
    pub tenant_id: Option<String>,
    pub project_id: Option<String>,
    pub thread_id: Option<String>,
    pub tags: serde_json::Map<String, serde_json::Value>,
    pub run_id: Option<String>,
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

pub type TraceMap = DashMap<TraceId, (broadcast::Sender<Span>, broadcast::Receiver<Span>)>;

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
                    KeyValue::new("langdb.tenant", tenant_name),
                    KeyValue::new("langdb.project_id", project_slug),
                ];

                let parent_trace_id = req
                    .headers()
                    .get("x-parent-trace-id")
                    .and_then(|v| v.to_str().ok().map(|v| v.to_string()));

                if let Some(parent_trace_id) = parent_trace_id.as_ref() {
                    key_values.push(KeyValue::new(
                        "langdb.parent_trace_id",
                        parent_trace_id.clone(),
                    ));
                }

                let trace_id = req
                    .headers()
                    .get("x-trace-id")
                    .and_then(|v| v.to_str().ok().map(|v| v.to_string()));

                if let Some(trace_id) = trace_id.as_ref() {
                    key_values.push(KeyValue::new("langdb.trace_id", trace_id.clone()));
                }

                let run_id = req
                    .headers()
                    .get("x-run-id")
                    .and_then(|v| v.to_str().ok().map(|v| v.to_string()));

                if let Some(run_id) = run_id.as_ref() {
                    key_values.push(KeyValue::new("langdb.run_id", run_id.clone()));
                } else {
                    key_values.push(KeyValue::new("langdb.run_id", Uuid::new_v4().to_string()));
                }

                let label = req
                    .headers()
                    .get("x-label")
                    .and_then(|v| v.to_str().ok().map(|v| v.to_string()));

                if let Some(label) = label.as_ref() {
                    key_values.push(KeyValue::new("langdb.label", label.clone()));
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
