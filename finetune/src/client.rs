use crate::types::{
    CreateDeploymentRequest, CreateEvaluationRequest, CreateEvaluationResponse,
    CreateFinetuneJobRequest, CreateJobRequest, CreateJobResponse, DatasetAnalyticsResponse,
    DeploymentResponse, DryRunDatasetAnalyticsRequest, DryRunDatasetAnalyticsResponse,
    DryRunEvaluatorRequest, DryRunEvaluatorResponse, EstimateJobResponse, EvaluationResultQuery,
    EvaluationResultResponse, EvaluationRunMetrics, EvaluatorVersionResponse,
    FinetuneEvalJobMetrics, FinetuneEvalResultsResponse, FinetuneJobInfraMetricsResponse,
    FinetuneJobMetricsResponse, FinetuneJobModelsResponse, FinetuneJobStatusResponse,
    FinetuningJobResponse, FinetuningJobResult, JobType, ReportFinetuneJobCheckpointStepRequest,
    UnifiedJobStatusResponse, UploadDatasetResponse, WeightsDownloadUrlResponse,
};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::de::DeserializeOwned;

/// Get cloud API URL
fn get_api_url() -> String {
    std::env::var("LANGDB_API_URL").unwrap_or_else(|_| "https://api.langdb.cloud".to_string())
}

/// Client for interacting with LangDB Cloud finetune API
pub struct LangdbCloudFinetuneClient {
    client: reqwest::Client,
    api_url: String,
}

impl LangdbCloudFinetuneClient {
    async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response, String> {
        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(format!("API error {}: {}", status, body))
    }

    async fn parse_json_response<T: DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<T, String> {
        let response = Self::ensure_success(response).await?;
        response
            .json::<T>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Create a new client with API key
    pub fn new(api_key: String) -> Result<Self, String> {
        let mut headers = HeaderMap::new();
        let authorization_value = format!("Bearer {}", api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&authorization_value)
                .map_err(|e| format!("Failed to create authorization header: {}", e))?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self {
            client,
            api_url: get_api_url(),
        })
    }

    /// Upload a dataset file (JSONL format).
    /// Use `append_mode=true` + `row_index_offset` for chunked uploads —
    /// first batch uses default (replace), subsequent batches append.
    pub async fn upload_dataset(
        &self,
        jsonl_data: Vec<u8>,
        topic_hierarchy: Option<String>,
        evaluator: Option<String>,
        dataset_id: Option<uuid::Uuid>,
    ) -> Result<UploadDatasetResponse, String> {
        self.upload_dataset_chunked(jsonl_data, topic_hierarchy, evaluator, dataset_id, false, 0)
            .await
    }

    /// Upload a dataset chunk. When `append_mode` is true, rows are upserted
    /// without deleting existing rows. `row_index_offset` shifts indices so
    /// appended batches get correct ordering.
    pub async fn upload_dataset_chunked(
        &self,
        jsonl_data: Vec<u8>,
        topic_hierarchy: Option<String>,
        evaluator: Option<String>,
        dataset_id: Option<uuid::Uuid>,
        append_mode: bool,
        row_index_offset: usize,
    ) -> Result<UploadDatasetResponse, String> {
        let mut form = reqwest::multipart::Form::new();

        let jsonl_part = reqwest::multipart::Part::bytes(jsonl_data)
            .file_name("training.jsonl")
            .mime_str("application/x-ndjson")
            .map_err(|e| format!("Failed to create multipart part: {}", e))?;
        form = form.part("file", jsonl_part);

        if let Some(topic_hierarchy) = topic_hierarchy {
            let topic_part = reqwest::multipart::Part::text(topic_hierarchy)
                .mime_str("application/json")
                .map_err(|e| format!("Failed to create topic_hierarchy part: {}", e))?;
            form = form.part("topic_hierarchy", topic_part);
        }

        if let Some(evaluator) = evaluator {
            let evaluator_part = reqwest::multipart::Part::text(evaluator)
                .mime_str("application/json")
                .map_err(|e| format!("Failed to create evaluator part: {}", e))?;
            form = form.part("evaluator", evaluator_part);
        }

        if let Some(dataset_id) = dataset_id {
            let dataset_id_part = reqwest::multipart::Part::text(dataset_id.to_string())
                .mime_str("text/plain")
                .map_err(|e| format!("Failed to create dataset_id part: {}", e))?;
            form = form.part("dataset_id", dataset_id_part);
        }

        if append_mode {
            form = form.part("append", reqwest::multipart::Part::text("true"));
            form = form.part(
                "row_index_offset",
                reqwest::multipart::Part::text(row_index_offset.to_string()),
            );
        }

        let url = format!("{}/finetune/workflows", self.api_url);

        let request = self.client.post(&url).multipart(form);

        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// List all evaluator versions for a dataset (newest to oldest)
    pub async fn get_workflow_evaluator_versions(
        &self,
        dataset_id: &str,
    ) -> Result<Vec<EvaluatorVersionResponse>, String> {
        let url = format!(
            "{}/finetune/workflows/{}/evaluator/versions",
            self.api_url, dataset_id
        );

        let req = self.client.get(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// Update the evaluator script for an existing dataset.
    pub async fn update_workflow_evaluator(
        &self,
        workflow_id: &str,
        eval_script: String,
    ) -> Result<(), String> {
        let url = format!(
            "{}/finetune/workflows/{}/evaluator",
            self.api_url, workflow_id
        );
        let script_part = reqwest::multipart::Part::text(eval_script)
            .file_name("evaluator.js")
            .mime_str("application/javascript")
            .map_err(|e| format!("Failed to create evaluator file part: {}", e))?;
        let form = reqwest::multipart::Form::new().part("file", script_part);
        let req = self.client.patch(&url).multipart(form);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        Self::ensure_success(response).await?;

        Ok(())
    }

    /// Create a new workflow. Returns the authoritative server-assigned UUID.
    /// Feature 001: `POST /finetune/workflows` with `{name, objective}` body.
    pub async fn create_workflow(
        &self,
        name: &str,
        objective: &str,
    ) -> Result<uuid::Uuid, String> {
        let url = format!("{}/finetune/workflows", self.api_url);
        let body = serde_json::json!({ "name": name, "objective": objective });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        let workflow: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        workflow
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .ok_or_else(|| format!("create_workflow: missing/invalid id in response: {}", workflow))
    }

    /// Mirror the local pipeline-journal state to the server. Best-effort write;
    /// the local file remains the source of truth.
    /// Feature 001: `PUT /finetune/workflows/{id}/pipeline-journal` — body is raw JSON.
    pub async fn put_pipeline_journal(
        &self,
        workflow_id: uuid::Uuid,
        journal_json: &str,
    ) -> Result<(), String> {
        let url = format!(
            "{}/finetune/workflows/{}/pipeline-journal",
            self.api_url, workflow_id
        );
        let response = self
            .client
            .put(&url)
            .header("content-type", "application/json")
            .body(journal_json.to_string())
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        Self::ensure_success(response).await?;
        Ok(())
    }

    /// Mirror the local iteration-state (analysis.json) to the server.
    /// Feature 001: `PUT /finetune/workflows/{id}/iteration-state` — body is raw JSON.
    pub async fn put_iteration_state(
        &self,
        workflow_id: uuid::Uuid,
        analysis_json: &str,
    ) -> Result<(), String> {
        let url = format!(
            "{}/finetune/workflows/{}/iteration-state",
            self.api_url, workflow_id
        );
        let response = self
            .client
            .put(&url)
            .header("content-type", "application/json")
            .body(analysis_json.to_string())
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        Self::ensure_success(response).await?;
        Ok(())
    }

    /// Cancel an in-progress evaluation run.
    /// `POST /finetune/workflows/evaluations/{id}/cancel`.
    pub async fn cancel_evaluation(&self, evaluation_run_id: &str) -> Result<(), String> {
        let url = format!(
            "{}/finetune/workflows/evaluations/{}/cancel",
            self.api_url, evaluation_run_id
        );
        let response = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        Self::ensure_success(response).await?;
        Ok(())
    }

    /// Unified API: create either a provider finetune or evaluation job.
    pub async fn create_job(
        &self,
        workflow_id: &uuid::Uuid,
        request: CreateJobRequest,
    ) -> Result<CreateJobResponse, String> {
        match request.job_type {
            JobType::Finetune => {
                let base_model = request.base_model.ok_or_else(|| {
                    "base_model is required for provider_finetune jobs".to_string()
                })?;

                let reinforcement_request = CreateFinetuneJobRequest {
                    evaluator_version: request.evaluator_version,
                    base_model,
                    output_model: request.output_model,
                    evaluation_dataset: request.evaluation_dataset,
                    display_name: request.display_name,
                    training_config: request.training_config,
                    inference_parameters: request.inference_parameters,
                    chunk_size: request.chunk_size,
                    node_count: request.node_count,
                    resume_mode: request.resume_mode,
                };

                let response = self
                    .create_finetune_job(reinforcement_request, workflow_id)
                    .await?;

                Ok(CreateJobResponse {
                    job_id: response.id,
                    job_type: JobType::Finetune,
                    status: response.status,
                    total_rows: None,
                })
            }
            JobType::EvaluationRun => {
                let rollout_model_params = request.rollout_model_params.ok_or_else(|| {
                    "rollout_model_params is required for evaluation_run jobs".to_string()
                })?;

                let evaluation_request = CreateEvaluationRequest {
                    workflow_id: *workflow_id,
                    rollout_model_params,
                    offset: request.offset,
                    limit: request.limit,
                };

                let response = self.create_evaluation(evaluation_request).await?;
                Ok(CreateJobResponse {
                    job_id: response.evaluation_run_id,
                    job_type: JobType::EvaluationRun,
                    status: response.status,
                    total_rows: Some(response.total_rows),
                })
            }
        }
    }

    /// Unified API: estimate either a provider finetune or evaluation job.
    pub async fn estimate_job(
        &self,
        workflow_id: &uuid::Uuid,
        request: Vec<CreateJobRequest>,
    ) -> Result<Vec<EstimateJobResponse>, String> {
        let url = format!("{}/finetune/jobs/{workflow_id}/estimate", self.api_url);
        let req = self.client.post(&url).json(&request);
        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;

        response
            .json::<Vec<EstimateJobResponse>>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Unified API: get status for provider/evaluation jobs.
    pub async fn get_job_status(
        &self,
        job_id: &str,
        job_type: JobType,
    ) -> Result<UnifiedJobStatusResponse, String> {
        match job_type {
            JobType::Finetune => {
                let status = self.get_finetune_job_status(job_id).await?;
                Ok(UnifiedJobStatusResponse {
                    job_id: status.provider_job_id,
                    job_type: JobType::Finetune,
                    status: status.status,
                    fine_tuned_model: status.fine_tuned_model,
                    error_message: status.error_message,
                    metrics: status.metrics,
                    request: status.request,
                    total_rows: None,
                    completed_rows: None,
                    failed_rows: None,
                })
            }
            JobType::EvaluationRun => {
                let result = self.get_evaluation_result(job_id, None).await?;
                Ok(UnifiedJobStatusResponse {
                    job_id: result.evaluation_run_id,
                    job_type: JobType::EvaluationRun,
                    status: result.status,
                    fine_tuned_model: None,
                    error_message: None,
                    metrics: None,
                    request: None,
                    total_rows: Some(result.total_rows),
                    completed_rows: Some(result.completed_rows),
                    failed_rows: Some(result.failed_rows),
                })
            }
        }
    }

    /// Create an evaluation run for a dataset
    pub async fn create_evaluation(
        &self,
        request: CreateEvaluationRequest,
    ) -> Result<CreateEvaluationResponse, String> {
        let url = format!("{}/finetune/workflows/evaluations", self.api_url);

        let req = self.client.post(&url).json(&request);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// Fetch finetune evaluation results grouped by row and epoch
    #[allow(clippy::too_many_arguments)]
    pub async fn get_finetune_evaluations(
        &self,
        dataset_id: &str,
        row_index: Option<i32>,
        epoch: Option<i32>,
        finetune_job_id: Option<String>,
        include_rollout_content: bool,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<FinetuneEvalResultsResponse, String> {
        let url = format!(
            "{}/finetune/workflows/{}/finetune-evaluations",
            self.api_url, dataset_id
        );

        let mut query_params: Vec<(&str, String)> = Vec::new();
        if let Some(ri) = row_index {
            query_params.push(("row_index", ri.to_string()));
        }
        if let Some(e) = epoch {
            query_params.push(("epoch", e.to_string()));
        }
        if let Some(job_id) = finetune_job_id {
            query_params.push(("finetune_job_id", job_id));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }
        if let Some(offset) = offset {
            query_params.push(("offset", offset.to_string()));
        }

        if include_rollout_content {
            query_params.push(("include_rollout_content", "true".to_string()));
        }

        let req = if query_params.is_empty() {
            self.client.get(&url)
        } else {
            self.client.get(&url).query(&query_params)
        };

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// Fetch aggregated finetune evaluation metrics for all jobs in a workflow.
    pub async fn get_finetune_evaluations_metrics(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<FinetuneEvalJobMetrics>, String> {
        let url = format!(
            "{}/finetune/workflows/{}/finetune-evaluations/metrics",
            self.api_url, workflow_id
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;

        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;
        serde_json::from_str::<Vec<FinetuneEvalJobMetrics>>(&body).map_err(|e| {
            let preview: String = body.chars().take(600).collect();
            format!("Failed to parse response: {}. body_preview={}", e, preview)
        })
    }

    /// Create a fine-tuning job
    pub async fn create_finetune_job(
        &self,
        request: CreateFinetuneJobRequest,
        workflow_id: &uuid::Uuid,
    ) -> Result<FinetuningJobResponse, String> {
        let url = format!("{}/finetune/jobs/{workflow_id}", self.api_url);

        let req = self.client.post(&url).json(&request);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;

        let text = response
            .text()
            .await
            .map_err(|e| format!("Failed to get response: {}", e))?;

        let body: FinetuningJobResult = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse response: {}. Response: {}", e, text))?;

        match body {
            FinetuningJobResult::Success(response) => Ok(*response),
            FinetuningJobResult::Error(error) => {
                Err(format!("Failed to create job: {}", error.message))
            }
        }
    }

    /// Get fine-tuning job status
    pub async fn get_finetune_job_status(
        &self,
        job_id: &str,
    ) -> Result<FinetuneJobStatusResponse, String> {
        let url = format!("{}/finetune/jobs/{}/status", self.api_url, job_id);

        let req = self.client.get(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    pub async fn cancel_finetune_job(&self, job_id: &str) -> Result<(), String> {
        let url = format!("{}/finetune/jobs/{}/cancel", self.api_url, job_id);

        let req = self.client.post(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        Self::ensure_success(response).await?;

        Ok(())
    }

    pub async fn resume_finetune_job(&self, job_id: &str) -> Result<(), String> {
        let url = format!("{}/finetune/jobs/{}/resume", self.api_url, job_id);

        let req = self.client.post(&url);

        let _response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        Ok(())
    }

    /// Get fine-tuning metrics for a job
    pub async fn get_finetune_job_metrics(
        &self,
        job_id: &str,
    ) -> Result<FinetuneJobMetricsResponse, String> {
        let url = format!("{}/finetune/jobs/{}/metrics", self.api_url, job_id);

        let req = self.client.get(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;
        Self::parse_json_response(response).await
    }

    /// Get infrastructure metrics for a finetune job (e.g. Vertex GPU utilization from Cloud Monitoring).
    pub async fn get_finetune_job_infra_metrics(
        &self,
        job_id: &str,
    ) -> Result<FinetuneJobInfraMetricsResponse, String> {
        let url = format!("{}/finetune/jobs/{}/infra-metrics", self.api_url, job_id);

        let req = self.client.get(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;
        Self::parse_json_response(response).await
    }

    pub async fn report_finetune_job_checkpoint_step(
        &self,
        job_id: &str,
        request: ReportFinetuneJobCheckpointStepRequest,
    ) -> Result<(), String> {
        let url = format!("{}/finetune/jobs/{}/checkpoint_step", self.api_url, job_id);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        Self::ensure_success(response).await?;

        Ok(())
    }

    /// Get available model aliases for a finetune job (checkpoints + finetuned model).
    pub async fn get_finetune_job_models(
        &self,
        job_id: &str,
    ) -> Result<FinetuneJobModelsResponse, String> {
        let url = format!("{}/finetune/jobs/{}/models", self.api_url, job_id);

        let req = self.client.get(&url);
        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;
        Self::parse_json_response(response).await
    }

    /// List fine-tuning jobs
    pub async fn list_finetune_jobs(
        &self,
        limit: Option<u32>,
        after: Option<String>,
    ) -> Result<Vec<FinetuningJobResponse>, String> {
        let mut url = format!("{}/finetune/jobs", self.api_url);
        let mut query_params = Vec::new();
        if let Some(limit) = limit {
            query_params.push(format!("limit={}", limit));
        }
        if let Some(after) = &after {
            query_params.push(format!("after={}", after));
        }
        if !query_params.is_empty() {
            url.push('?');
            url.push_str(&query_params.join("&"));
        }

        let req = self.client.get(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// Get evaluation results for a given evaluation run
    pub async fn get_evaluation_result(
        &self,
        evaluation_run_id: &str,
        query: Option<EvaluationResultQuery>,
    ) -> Result<EvaluationResultResponse, String> {
        let url = format!(
            "{}/finetune/workflows/evaluations/{}",
            self.api_url, evaluation_run_id
        );

        let mut query_params: Vec<(&str, String)> = Vec::new();
        if let Some(query) = query {
            if let Some(limit) = query.limit {
                query_params.push(("limit", limit.to_string()));
            }
            if let Some(offset) = query.offset {
                query_params.push(("offset", offset.to_string()));
            }
            if let Some(sort) = query.sort {
                query_params.push(("sort", sort));
            }
            if let Some(order) = query.order {
                query_params.push(("order", order));
            }
            if let Some(sort_by) = query.sort_by {
                query_params.push(("sort_by", sort_by));
            }
        }

        let req = if query_params.is_empty() {
            self.client.get(&url)
        } else {
            self.client.get(&url).query(&query_params)
        };

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// Get aggregated metrics for all evaluation runs of a workflow
    pub async fn get_workflow_evaluation_metrics(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<EvaluationRunMetrics>, String> {
        let url = format!(
            "{}/finetune/workflows/{}/evaluations/metrics",
            self.api_url, workflow_id
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;

        response
            .json::<Vec<EvaluationRunMetrics>>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Get analytics for a given dataset (structure + quality)
    pub async fn get_dataset_analytics(
        &self,
        dataset_id: &str,
    ) -> Result<DatasetAnalyticsResponse, String> {
        let url = format!(
            "{}/finetune/workflows/{}/analytics",
            self.api_url, dataset_id
        );

        let req = self.client.get(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// Compute analytics for an in-memory dataset (not uploaded).
    pub async fn dry_run_dataset_analytics(
        &self,
        request: DryRunDatasetAnalyticsRequest,
    ) -> Result<DryRunDatasetAnalyticsResponse, String> {
        let url = format!("{}/finetune/workflows/analytics/dry-run", self.api_url);

        let req = self.client.post(&url).json(&request);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// Run a workflow evaluator script immediately without saving it.
    pub async fn dry_run_workflow_evaluator(
        &self,
        workflow_id: &str,
        request: DryRunEvaluatorRequest,
    ) -> Result<DryRunEvaluatorResponse, String> {
        let url = format!(
            "{}/finetune/workflows/{}/evaluator/dry-run",
            self.api_url, workflow_id
        );

        let mut form = reqwest::multipart::Form::new();
        let script_part = reqwest::multipart::Part::text(request.script)
            .mime_str("text/plain")
            .map_err(|e| format!("Failed to create script part: {}", e))?;
        form = form.part("script", script_part);

        if let Some(row) = request.row {
            let row_part = reqwest::multipart::Part::text(row.to_string())
                .mime_str("application/json")
                .map_err(|e| format!("Failed to create row part: {}", e))?;
            form = form.part("row", row_part);
        }

        let req = self.client.post(&url).multipart(form);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// Deploy a fine-tuned model
    pub async fn deploy_model(
        &self,
        request: CreateDeploymentRequest,
    ) -> Result<DeploymentResponse, String> {
        let url = format!("{}/finetune/deployments", self.api_url);

        let req = self.client.post(&url).json(&request);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }

    /// Delete a deployment
    pub async fn delete_deployment(&self, deployment_id: &str) -> Result<(), String> {
        let url = format!("{}/finetune/deployments/{}", self.api_url, deployment_id);

        let req = self.client.delete(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        Self::ensure_success(response).await?;

        Ok(())
    }

    /// Get a signed URL to download trained weights for a completed fine-tuning job
    pub async fn get_weights_download_url(
        &self,
        job_id: &str,
    ) -> Result<WeightsDownloadUrlResponse, String> {
        let url = format!(
            "{}/finetune/reinforcement-jobs/{}/weights/url",
            self.api_url, job_id
        );

        let req = self.client.get(&url);

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to call cloud API: {}", e))?;

        let response = Self::ensure_success(response).await?;
        Self::parse_json_response(response).await
    }
}
