use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Response types
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadDatasetResponse {
    pub dataset_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReinforcementFinetuningJobRequest {
    pub dataset: String,
    pub base_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation_dataset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_config: Option<ReinforcementTrainingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_parameters: Option<ReinforcementInferenceParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReinforcementTrainingConfig {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReinforcementInferenceParameters {
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinetuningJobResponse {
    pub id: uuid::Uuid,
    pub provider_job_id: String,
    pub status: String,
    pub base_model: String,
    pub fine_tuned_model: Option<String>,
    pub provider: String,
    pub hyperparameters: Option<serde_json::Value>,
    pub suffix: Option<String>,
    pub error_message: Option<String>,
    pub training_file_id: String,
    pub validation_file_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReinforcementJobStatusResponse {
    pub provider_job_id: String,
    pub status: String,
    pub fine_tuned_model: Option<String>,
    pub error_message: Option<String>,
}

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
    pub inference_model_name: Option<String>,
}
