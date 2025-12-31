use crate::serialize_any_value;
use opentelemetry_proto::tonic::collector::metrics::v1::{
    metrics_service_server::MetricsService, ExportMetricsPartialSuccess,
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use opentelemetry_proto::tonic::metrics::v1 as metrics_proto;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use crate::TraceTenantResolver;

#[async_trait::async_trait]
pub trait MetricsWriterTransport: Send + Sync {
    async fn write_metrics(
        &self,
        metrics: Vec<MetricsDataPoint>,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>>;
}

#[derive(Clone, Debug)]
pub struct MetricsDataPoint {
    pub metric_name: String,
    pub metric_type: String,
    pub value: f64,
    pub timestamp_us: i64,
    pub attributes: HashMap<String, Value>,
    pub project_id: Option<String>,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

pub struct MetricsServiceImpl {
    writer: Arc<dyn MetricsWriterTransport>,
    tenant_resolver: Box<dyn TraceTenantResolver>,
}

impl std::fmt::Debug for MetricsServiceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsServiceImpl").finish()
    }
}

impl MetricsServiceImpl {
    pub fn new(
        writer: Arc<dyn MetricsWriterTransport>,
        tenant_resolver: Box<dyn TraceTenantResolver>,
    ) -> Self {
        Self {
            writer,
            tenant_resolver,
        }
    }

    fn extract_context_from_attributes(
        attributes: &HashMap<String, Value>,
    ) -> (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) {
        let project_id = attributes
            .get("project_id")
            .or_else(|| attributes.get("vllora.project_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let thread_id = attributes
            .get("thread_id")
            .or_else(|| attributes.get("vllora.thread_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let run_id = attributes
            .get("run_id")
            .or_else(|| attributes.get("vllora.run_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let trace_id = attributes
            .get("trace_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let span_id = attributes
            .get("span_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        (project_id, thread_id, run_id, trace_id, span_id)
    }

    fn convert_attributes_to_json(
        attributes: &HashMap<String, Value>,
    ) -> HashMap<String, serde_json::Value> {
        attributes.clone()
    }

    fn convert_metric_data_point_to_metrics(
        &self,
        metric_name: &str,
        metric_type: &str,
        data_point: &metrics_proto::NumberDataPoint,
        resource_attrs: &HashMap<String, Value>,
    ) -> Result<Vec<MetricsDataPoint>, String> {
        let mut metrics = Vec::new();

        let timestamp_us = (data_point.time_unix_nano / 1_000) as i64;

        // Merge data point attributes with resource attributes
        let mut all_attrs = resource_attrs.clone();
        for attr in &data_point.attributes {
            let key = attr.key.clone();
            let value = serialize_any_value(attr.value.clone().unwrap_or_default());
            all_attrs.insert(key, value);
        }

        let (project_id, thread_id, run_id, trace_id, span_id) =
            Self::extract_context_from_attributes(&all_attrs);
        let attributes_json = Self::convert_attributes_to_json(&all_attrs);

        let value = match data_point.value.as_ref() {
            Some(metrics_proto::number_data_point::Value::AsInt(i)) => *i as f64,
            Some(metrics_proto::number_data_point::Value::AsDouble(f)) => *f,
            None => 0.0,
        };

        metrics.push(MetricsDataPoint {
            metric_name: metric_name.to_string(),
            metric_type: metric_type.to_string(),
            value,
            timestamp_us,
            attributes: attributes_json,
            project_id,
            thread_id,
            run_id,
            trace_id,
            span_id,
        });

        Ok(metrics)
    }

    fn convert_histogram_data_point_to_metrics(
        &self,
        metric_name: &str,
        metric_type: &str,
        data_point: &metrics_proto::HistogramDataPoint,
        resource_attrs: &HashMap<String, Value>,
    ) -> Result<Vec<MetricsDataPoint>, String> {
        let mut metrics = Vec::new();

        let timestamp_us = (data_point.time_unix_nano / 1_000) as i64;

        // Merge data point attributes with resource attributes
        let mut all_attrs = resource_attrs.clone();
        for attr in &data_point.attributes {
            let key = attr.key.clone();
            let value = serialize_any_value(attr.value.clone().unwrap_or_default());
            all_attrs.insert(key, value);
        }

        let (project_id, thread_id, run_id, trace_id, span_id) =
            Self::extract_context_from_attributes(&all_attrs);
        let attributes_json = Self::convert_attributes_to_json(&all_attrs);

        // Store count
        let count_value = data_point.count as f64;
        metrics.push(MetricsDataPoint {
            metric_name: metric_name.to_string(),
            metric_type: metric_type.to_string(),
            value: count_value,
            timestamp_us,
            attributes: attributes_json.clone(),
            project_id: project_id.clone(),
            thread_id: thread_id.clone(),
            run_id: run_id.clone(),
            trace_id: trace_id.clone(),
            span_id: span_id.clone(),
        });

        // Store sum if available
        if let Some(sum) = data_point.sum {
            let sum_metric_name = format!("{}.sum", metric_name);
            metrics.push(MetricsDataPoint {
                metric_name: sum_metric_name,
                metric_type: metric_type.to_string(),
                value: sum,
                timestamp_us,
                attributes: attributes_json,
                project_id,
                thread_id,
                run_id,
                trace_id,
                span_id,
            });
        }

        Ok(metrics)
    }

    fn convert_gauge_data_point_to_metrics(
        &self,
        metric_name: &str,
        metric_type: &str,
        data_point: &metrics_proto::NumberDataPoint,
        resource_attrs: &HashMap<String, Value>,
    ) -> Result<Vec<MetricsDataPoint>, String> {
        let mut metrics = Vec::new();

        let timestamp_us = (data_point.time_unix_nano / 1_000) as i64;

        // Merge data point attributes with resource attributes
        let mut all_attrs = resource_attrs.clone();
        for attr in &data_point.attributes {
            let key = attr.key.clone();
            let value = serialize_any_value(attr.value.clone().unwrap_or_default());
            all_attrs.insert(key, value);
        }

        let (project_id, thread_id, run_id, trace_id, span_id) =
            Self::extract_context_from_attributes(&all_attrs);
        let attributes_json = Self::convert_attributes_to_json(&all_attrs);

        let value = match data_point.value.as_ref() {
            Some(metrics_proto::number_data_point::Value::AsInt(i)) => *i as f64,
            Some(metrics_proto::number_data_point::Value::AsDouble(f)) => *f,
            None => 0.0,
        };

        metrics.push(MetricsDataPoint {
            metric_name: metric_name.to_string(),
            metric_type: metric_type.to_string(),
            value,
            timestamp_us,
            attributes: attributes_json,
            project_id,
            thread_id,
            run_id,
            trace_id,
            span_id,
        });

        Ok(metrics)
    }

    async fn convert_resource_metrics_to_metrics(
        &self,
        resource_metrics: metrics_proto::ResourceMetrics,
        tenant_from_header: Option<(String, String)>,
    ) -> Result<Vec<MetricsDataPoint>, String> {
        let mut all_metrics = Vec::new();

        // Extract resource attributes
        let mut resource_attrs: HashMap<String, Value> = resource_metrics
            .resource
            .as_ref()
            .map(|r| {
                r.attributes
                    .iter()
                    .map(|attr| {
                        (
                            attr.key.clone(),
                            serialize_any_value(attr.value.clone().unwrap_or_default()),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Add tenant/project from header if available
        if let Some((tenant_id, project_id)) = tenant_from_header {
            resource_attrs.insert("vllora.tenant".to_string(), tenant_id.into());
            resource_attrs.insert("vllora.project_id".to_string(), project_id.into());
        }

        for scope_metrics in resource_metrics.scope_metrics {
            for metric in scope_metrics.metrics {
                let metric_name = metric.name.clone();
                let metric_type = match metric.data.as_ref() {
                    Some(metrics_proto::metric::Data::Gauge(_)) => "gauge",
                    Some(metrics_proto::metric::Data::Sum(_)) => "counter",
                    Some(metrics_proto::metric::Data::Histogram(_)) => "histogram",
                    Some(metrics_proto::metric::Data::ExponentialHistogram(_)) => "histogram",
                    Some(metrics_proto::metric::Data::Summary(_)) => "summary",
                    None => "unknown",
                };

                match metric.data.as_ref() {
                    Some(metrics_proto::metric::Data::Sum(sum)) => {
                        for data_point in &sum.data_points {
                            all_metrics.extend(
                                self.convert_metric_data_point_to_metrics(
                                    &metric_name,
                                    metric_type,
                                    data_point,
                                    &resource_attrs,
                                )?,
                            );
                        }
                    }
                    Some(metrics_proto::metric::Data::Gauge(gauge)) => {
                        for data_point in &gauge.data_points {
                            all_metrics.extend(
                                self.convert_gauge_data_point_to_metrics(
                                    &metric_name,
                                    metric_type,
                                    data_point,
                                    &resource_attrs,
                                )?,
                            );
                        }
                    }
                    Some(metrics_proto::metric::Data::Histogram(histogram)) => {
                        for data_point in &histogram.data_points {
                            all_metrics.extend(
                                self.convert_histogram_data_point_to_metrics(
                                    &metric_name,
                                    metric_type,
                                    data_point,
                                    &resource_attrs,
                                )?,
                            );
                        }
                    }
                    _ => {
                        tracing::debug!("Unsupported metric type: {:?}", metric.data);
                    }
                }
            }
        }

        Ok(all_metrics)
    }
}

#[tonic::async_trait]
impl MetricsService for MetricsServiceImpl {
    #[tracing::instrument(level = "info")]
    async fn export(
        &self,
        request: tonic::Request<ExportMetricsServiceRequest>,
    ) -> tonic::Result<tonic::Response<ExportMetricsServiceResponse>> {
        let mut rejected = 0;
        let mut all_metrics = Vec::new();

        let headers = request.metadata();
        let tenant_from_header = self.tenant_resolver.get_tenant_id(headers).await;

        for resource_metrics in request.into_inner().resource_metrics {
            match self
                .convert_resource_metrics_to_metrics(resource_metrics, tenant_from_header.clone())
                .await
            {
                Ok(mut metrics) => {
                    all_metrics.append(&mut metrics);
                }
                Err(e) => {
                    tracing::error!("Failed to convert metrics: {}", e);
                    rejected += 1;
                }
            }
        }

        // Write all metrics to database
        let metrics_count = all_metrics.len();
        if !all_metrics.is_empty() {
            match self.writer.write_metrics(all_metrics).await {
                Ok(count) => {
                    tracing::debug!("Wrote {} metrics to database", count);
                }
                Err(e) => {
                    tracing::error!("Failed to write metrics to database: {}", e);
                    rejected += metrics_count as i64;
                }
            }
        }

        Ok(tonic::Response::new(ExportMetricsServiceResponse {
            partial_success: Some(ExportMetricsPartialSuccess {
                rejected_data_points: rejected,
                error_message: "".into(),
            }),
        }))
    }
}
