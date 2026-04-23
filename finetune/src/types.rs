use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Base model load precision for training (Vertex / Unsloth / HF loaders).
/// JSON values: `bf16`, `4bit`, `8bit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadPrecision {
    Bf16,
    FourBit,
    EightBit,
}

impl Serialize for LoadPrecision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            LoadPrecision::Bf16 => "bf16",
            LoadPrecision::FourBit => "4bit",
            LoadPrecision::EightBit => "8bit",
        })
    }
}

impl<'de> Deserialize<'de> for LoadPrecision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "bf16" => Ok(LoadPrecision::Bf16),
            "4bit" => Ok(LoadPrecision::FourBit),
            "8bit" => Ok(LoadPrecision::EightBit),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &["bf16", "4bit", "8bit"],
            )),
        }
    }
}

/// TRL `GRPOConfig.loss_type` values (Dr. GRPO, GRPO, DAPO, BNPO).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GrpoLossType {
    DrGrpo,
    Grpo,
    Dapo,
    Bnpo,
}

/// TRL `GRPOConfig.importance_sampling_level`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImportanceSamplingLevel {
    Token,
    Sequence,
}

/// TRL `GRPOConfig.scale_rewards`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScaleRewards {
    Group,
    Batch,
    None,
}

// =============================================================================
// Unified Jobs
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    #[serde(alias = "provider_finetune")]
    Finetune,
    EvaluationRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BaseModel {
    #[serde(rename = "Qwen3.5-0.8B", alias = "unsloth/Qwen3.5-0.8B")]
    Qwen35_0_8B,
    #[serde(rename = "Qwen3.5-2B", alias = "unsloth/Qwen3.5-2B")]
    Qwen35_2B,
    #[serde(rename = "Qwen3.5-4B", alias = "unsloth/Qwen3.5-4B")]
    Qwen35_4B,
    #[serde(alias = "gemma-4-E2B")]
    Gemma4E2B,
}

impl BaseModel {
    pub fn as_str(&self) -> &'static str {
        match self {
            BaseModel::Qwen35_0_8B => "Qwen3.5-0.8B",
            BaseModel::Qwen35_2B => "Qwen3.5-2B",
            BaseModel::Qwen35_4B => "Qwen3.5-4B",
            BaseModel::Gemma4E2B => "gemma-4-E2B",
        }
    }
}

impl std::fmt::Display for BaseModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobRequest {
    pub job_type: JobType,

    /// Optional caller-provided idempotency key (redesign §2.4). When set,
    /// the server dedups by `(workflow_id, key, payload_hash)` — replaying
    /// the same request returns the original `job_id`. Different payloads
    /// under the same key return `409 CONFLICT`. Absent keys cause the
    /// server to generate one and echo it in the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,

    // Provider finetune payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluator_version: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_model: Option<BaseModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation_dataset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_config: Option<FinetuneTrainingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_parameters: Option<FinetuneInferenceParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_mode: Option<ResumeMode>,

    // Evaluation payload
    #[serde(alias = "model_params", alias = "rollout_model_parameters")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollout_model_params: Option<CompletionParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ResumeMode {
    FullState,
    WeightsOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobResponse {
    pub job_id: uuid::Uuid,
    pub job_type: JobType,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_rows: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimateJobVariant {
    pub workflow_id: uuid::Uuid,
    pub job_type: JobType,
    pub instance: String,
    pub base_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_rows: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_duration_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimateJobResponse {
    pub config_index: usize,
    pub estimations: Vec<EstimateJobVariant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedJobStatusResponse {
    pub job_id: uuid::Uuid,
    pub job_type: JobType,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fine_tuned_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_rows: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_rows: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_rows: Option<i32>,
}

// =============================================================================
// Dataset
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadDatasetResponse {
    pub dataset_id: uuid::Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatasetAnalyticsResponse {
    pub dataset_id: uuid::Uuid,
    pub analytics: JsonValue,
    pub quality: JsonValue,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DryRunDatasetAnalyticsRequest {
    pub rows: Vec<JsonValue>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DryRunDatasetAnalyticsResponse {
    pub analytics: JsonValue,
    pub quality: JsonValue,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DryRunEvaluatorRequest {
    pub script: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunEvaluatorResponse {
    pub score: f64,
    #[serde(alias = "reasoning")]
    pub reason: String,
    #[serde(default)]
    pub logs: Vec<String>,
    #[serde(flatten)]
    pub other: HashMap<String, JsonValue>,
    #[serde(default)]
    pub is_success: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateEvaluatorBody {
    pub eval_script: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateEvaluatorResponse {
    pub workflow_id: uuid::Uuid,
    pub updated: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluatorVersionResponse {
    pub id: uuid::Uuid,
    pub workflow_id: uuid::Uuid,
    pub version: i32,
    pub config: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// Fine-Tuning Jobs
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFinetuneJobRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluator_version: Option<i32>,
    pub base_model: BaseModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation_dataset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_config: Option<FinetuneTrainingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_parameters: Option<FinetuneInferenceParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_mode: Option<ResumeMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneTrainingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learning_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_context_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora_rank: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epochs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_accumulation_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learning_rate_warmup_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size_samples: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_precision: Option<LoadPrecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask_truncated_completions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loss_type: Option<GrpoLossType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance_sampling_level: Option<ImportanceSamplingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_rewards: Option<ScaleRewards>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beta: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneInferenceParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_candidates_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinetuningJobResponse {
    pub id: uuid::Uuid,
    pub status: String,
    pub base_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fine_tuned_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_config: Option<FinetuneTrainingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub training_file_id: uuid::Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_file_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinetuningJobError {
    pub code: u16,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FinetuningJobResult {
    Success(Box<FinetuningJobResponse>),
    Error(FinetuningJobError),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinetuneJobStatusResponse {
    pub provider_job_id: uuid::Uuid,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fine_tuned_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneJobMetricPoint {
    pub metrics: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneJobMetricsResponse {
    pub provider_job_id: uuid::Uuid,
    pub metrics: Vec<FinetuneJobMetricPoint>,
}

/// Vertex / provider infrastructure metrics (GPU utilization, memory, etc.) from Cloud Monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneJobInfraMetricPoint {
    pub metric_type: String,
    pub metric_time: chrono::DateTime<chrono::Utc>,
    pub metric_value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneJobInfraMetricsResponse {
    pub provider_job_id: uuid::Uuid,
    pub metrics: Vec<FinetuneJobInfraMetricPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneJobModelsResponse {
    pub provider_job_id: uuid::Uuid,
    #[serde(default)]
    pub checkpoints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_checkpoint_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finetuned_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportFinetuneJobMetricsRequest {
    pub metrics: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportFinetuneJobCheckpointStepRequest {
    pub checkpoint_step: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneEpochScore {
    pub epoch: i32,
    pub avg_score: f64,
}

fn deserialize_avg_score_by_epoch<'de, D>(
    deserializer: D,
) -> Result<Vec<FinetuneEpochScore>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut list = Vec::<FinetuneEpochScore>::deserialize(deserializer)?;
    list.sort_by_key(|entry| entry.epoch);
    Ok(list)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneJobEvalMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_epoch_with_score: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_score: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_avg_score_by_epoch")]
    pub avg_score_by_epoch: Vec<FinetuneEpochScore>,
    #[serde(default)]
    pub distinct_rows_with_eval: usize,
}

#[derive(Debug, Deserialize)]
pub struct FinetuneJobQuery {
    pub limit: Option<u32>,
    pub after: Option<String>,
    pub dataset_id: Option<String>,
    pub include_metrics: Option<bool>,
}

// =============================================================================
// Deployments
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDeploymentRequest {
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_replica_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_replica_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autoscaling_policy: Option<AutoscalingPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoscalingPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_up_window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_down_window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_to_zero_window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_targets: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeploymentResponse {
    pub deployment_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_model_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WeightsDownloadUrlResponse {
    pub download_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

// =============================================================================
// Evaluations
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionParams {
    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub temperature: Option<f64>,

    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEvaluationRequest {
    #[serde(alias = "dataset_id")]
    pub workflow_id: uuid::Uuid,
    #[serde(alias = "model_params")]
    pub rollout_model_params: CompletionParams,
    pub offset: Option<i32>,
    pub limit: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateEvaluationResponse {
    pub evaluation_run_id: uuid::Uuid,
    pub status: String,
    pub total_rows: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluationSummary {
    pub average_score: Option<f64>,
    #[serde(default)]
    pub passed_count: i32,
    #[serde(default)]
    pub failed_count: i32,
    /// Number of rows with a non-null score (scored_count <= completed_rows)
    #[serde(default)]
    pub scored_count: i32,
    /// Rows with score < 0.01 — detects broken graders that can't parse model output
    #[serde(default)]
    pub zero_score_count: i32,
    /// Rows with score >= 0.99 — detects overly lenient graders
    #[serde(default)]
    pub perfect_score_count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluationResultResponse {
    pub evaluation_run_id: uuid::Uuid,
    pub status: String,
    #[serde(default)]
    pub total_rows: i32,
    #[serde(default)]
    pub completed_rows: i32,
    #[serde(default)]
    pub failed_rows: i32,
    #[serde(default)]
    pub results: Vec<RowEpochResults>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<EvaluationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationRunMetrics {
    pub evaluation_run_id: uuid::Uuid,
    pub status: String,
    #[serde(default)]
    pub total_rows: i32,
    #[serde(default)]
    pub completed_rows: i32,
    #[serde(default)]
    pub failed_rows: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_stddev: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_score: Option<f64>,
    #[serde(default)]
    pub scored_count: usize,
    #[serde(default)]
    pub passed_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResultQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort: Option<String>,
    pub order: Option<String>,
    // Backward compatibility for older clients using score_asc/score_desc
    pub sort_by: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RowEpochResults {
    pub row_index: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<JsonValue>,
    #[serde(default)]
    pub epochs: HashMap<String, Vec<JsonValue>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinetuneEvalResultsResponse {
    pub results: Vec<RowEpochResults>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneEvalJobMetrics {
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_epoch: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_score: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_avg_score_by_epoch")]
    pub avg_score_by_epoch: Vec<FinetuneEpochScore>,
    #[serde(default)]
    pub distinct_rows_with_eval: usize,
}

#[derive(Debug, Deserialize)]
pub struct FinetuneEvalQuery {
    pub row_index: Option<i32>,
    pub epoch: Option<i32>,
    pub finetune_job_id: Option<uuid::Uuid>,
    pub include_rollout_content: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// =============================================================================
// Evaluator Configuration
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
#[serde(bound(deserialize = "T: serde::de::DeserializeOwned"))]
pub enum Evaluator<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + Clone,
{
    LlmAsJudge { config: LlmAsJudgeConfig<T> },
    Js { config: JsConfig },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: serde::de::DeserializeOwned"))]
pub struct EvaluatorWithVersion<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + Clone,
{
    pub evaluator: Evaluator<T>,
    pub version: i32,
}

impl<T> EvaluatorWithVersion<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + Clone,
{
    pub fn new(evaluator: Evaluator<T>, version: i32) -> Self {
        Self { evaluator, version }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: serde::de::DeserializeOwned"))]
pub struct LlmAsJudgeConfig<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug + Clone,
{
    pub prompt_template: Vec<T>,
    pub output_schema: serde_json::Value,
    pub completion_params: CompletionModelParams,
    pub score_formula: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompletionModelParams {
    pub model_name: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct JsConfig {
    pub script: Option<String>,
}

// =============================================================================
// Canonical Layer B job-lifecycle shapes
// (specs/001-job-based-cli-api/contracts/jobs.openapi.yaml)
// =============================================================================

/// Every Layer B operation surfaced by the workflow-scoped jobs route.
/// SCREAMING_SNAKE on the wire matches the OpenAPI `OperationKey` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationKey {
    KnowledgeAdd,
    RecordsImport,
    RecordsGenerate,
    GraderImport,
    GraderGenerate,
    GraderDryrun,
    EvalRun,
    TrainRun,
    TestJob,
}

/// Job lifecycle states matching OpenAPI `JobState`. Same semantics as the
/// `JobState` used by the GatewayClient trait; kept separate so the wire
/// format (lowercase strings) stays fixed even if the internal trait's
/// variants drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobLifecycleState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// Who initiated the job. OpenAPI contract: required `{type, id}` plus
/// optional `display_name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Initiator {
    #[serde(rename = "type")]
    pub initiator_type: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// `POST /v1/finetune/workflows/{workflowId}/jobs` request body. This is
/// the spec-canonical shape; the existing `CreateJobRequest` remains the
/// concrete payload, carried in `input` so callers that already built
/// against it can adapt incrementally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStartRequest {
    pub operation: OperationKey,
    /// Optional operation-specific payload. Provisional per the OpenAPI
    /// spec — currently accepted but not semantically validated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    pub initiator: Initiator,
    /// Optional caller-provided deduplication key. If omitted, the server
    /// generates one and returns it via `idempotency_key` in the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// Snapshot of job progress. Optional fields per OpenAPI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgressSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_time: Option<chrono::DateTime<chrono::Utc>>,
}

/// `202 Accepted` response from `POST /jobs` — matches OpenAPI
/// `JobStartResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStartResponse {
    pub job_id: String,
    pub workflow_id: String,
    pub operation: OperationKey,
    pub idempotency_key: String,
    /// `true` once the row is committed to the DB (so callers can
    /// distinguish accepted-but-not-persisted from fully durable).
    pub persisted: bool,
    pub state: JobLifecycleState,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<JobProgressSnapshot>,
}

#[cfg(test)]
mod canonical_job_tests {
    use super::*;

    #[test]
    fn operation_key_wire_strings_match_openapi() {
        // Must exactly match `jobs.openapi.yaml § OperationKey`.
        let matrix = [
            (OperationKey::KnowledgeAdd, "\"knowledge-add\""),
            (OperationKey::RecordsImport, "\"records-import\""),
            (OperationKey::RecordsGenerate, "\"records-generate\""),
            (OperationKey::GraderImport, "\"grader-import\""),
            (OperationKey::GraderGenerate, "\"grader-generate\""),
            (OperationKey::GraderDryrun, "\"grader-dryrun\""),
            (OperationKey::EvalRun, "\"eval-run\""),
            (OperationKey::TrainRun, "\"train-run\""),
            (OperationKey::TestJob, "\"test-job\""),
        ];
        for (variant, wire) in matrix {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, wire, "wire mismatch for {variant:?}");
        }
    }

    #[test]
    fn job_lifecycle_state_wire_strings_match_openapi() {
        let matrix = [
            (JobLifecycleState::Queued, "\"queued\""),
            (JobLifecycleState::Running, "\"running\""),
            (JobLifecycleState::Succeeded, "\"succeeded\""),
            (JobLifecycleState::Failed, "\"failed\""),
            (JobLifecycleState::Cancelled, "\"cancelled\""),
        ];
        for (variant, wire) in matrix {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, wire, "wire mismatch for {variant:?}");
        }
    }

    #[test]
    fn job_start_request_parses_canonical_body() {
        // Body shape from `jobs.openapi.yaml § JobStartRequest`.
        let raw = r#"{
            "operation": "eval-run",
            "input": {"model": "qwen-4b"},
            "initiator": {
                "type": "user",
                "id": "u-123",
                "display_name": "Daisy"
            },
            "idempotency_key": "eval-run-2026-04-23"
        }"#;
        let req: JobStartRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.operation, OperationKey::EvalRun);
        assert_eq!(req.initiator.initiator_type, "user");
        assert_eq!(req.initiator.id, "u-123");
        assert_eq!(req.initiator.display_name.as_deref(), Some("Daisy"));
        assert_eq!(req.idempotency_key.as_deref(), Some("eval-run-2026-04-23"));
    }

    #[test]
    fn job_start_response_round_trips_through_json() {
        let ts = chrono::Utc::now();
        let resp = JobStartResponse {
            job_id: "job-xyz".into(),
            workflow_id: "wf-abc".into(),
            operation: OperationKey::TrainRun,
            idempotency_key: "idem-1".into(),
            persisted: true,
            state: JobLifecycleState::Queued,
            created_at: ts,
            updated_at: ts,
            progress: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: JobStartResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.operation, OperationKey::TrainRun);
        assert_eq!(decoded.state, JobLifecycleState::Queued);
        assert!(decoded.persisted);
    }
}
