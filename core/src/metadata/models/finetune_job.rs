use crate::metadata::schema::finetune_jobs;
use diesel::{AsChangeset, Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Finetune job state enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FinetuneJobState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl std::fmt::Display for FinetuneJobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FinetuneJobState::Pending => write!(f, "pending"),
            FinetuneJobState::Running => write!(f, "running"),
            FinetuneJobState::Succeeded => write!(f, "succeeded"),
            FinetuneJobState::Failed => write!(f, "failed"),
            FinetuneJobState::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for FinetuneJobState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(FinetuneJobState::Pending),
            "running" => Ok(FinetuneJobState::Running),
            "succeeded" => Ok(FinetuneJobState::Succeeded),
            "failed" => Ok(FinetuneJobState::Failed),
            "cancelled" => Ok(FinetuneJobState::Cancelled),
            _ => Err(format!("Invalid finetune job state: {}", s)),
        }
    }
}

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = finetune_jobs)]
#[serde(crate = "serde")]
pub struct DbFinetuneJob {
    pub id: String,
    pub project_id: String,
    pub dataset_id: String,
    pub state: String, // Stored as lowercase string: "pending", "running", etc.
    pub provider: String,
    pub provider_job_id: String,
    pub base_model: String,
    pub fine_tuned_model: Option<String>,
    pub error_message: Option<String>,
    pub hyperparameters: Option<String>, // JSON stored as text
    pub training_file_id: Option<String>,
    pub validation_file_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

impl DbFinetuneJob {
    /// Get the job state as an enum
    pub fn state_enum(&self) -> Result<FinetuneJobState, String> {
        self.state.parse()
    }

    /// Set the job state from an enum
    pub fn set_state(&mut self, state: FinetuneJobState) {
        self.state = state.to_string();
    }
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = finetune_jobs)]
pub struct DbNewFinetuneJob {
    pub id: Option<String>,
    pub project_id: String,
    pub dataset_id: String,
    pub state: String,
    pub provider: String,
    pub provider_job_id: String,
    pub base_model: String,
    pub fine_tuned_model: Option<String>,
    pub error_message: Option<String>,
    pub hyperparameters: Option<String>,
    pub training_file_id: Option<String>,
    pub validation_file_id: Option<String>,
}

impl DbNewFinetuneJob {
    pub fn new(
        project_id: String,
        dataset_id: String,
        provider: String,
        provider_job_id: String,
        base_model: String,
    ) -> Self {
        Self {
            id: None, // Will use default UUID generation
            project_id,
            dataset_id,
            state: "pending".to_string(),
            provider,
            provider_job_id,
            base_model,
            fine_tuned_model: None,
            error_message: None,
            hyperparameters: None,
            training_file_id: None,
            validation_file_id: None,
        }
    }

    pub fn with_fine_tuned_model(mut self, fine_tuned_model: Option<String>) -> Self {
        self.fine_tuned_model = fine_tuned_model;
        self
    }

    pub fn with_error_message(mut self, error_message: Option<String>) -> Self {
        self.error_message = error_message;
        self
    }

    pub fn with_hyperparameters(
        mut self,
        hyperparameters: Option<HashMap<String, serde_json::Value>>,
    ) -> Self {
        self.hyperparameters = hyperparameters.and_then(|h| serde_json::to_string(&h).ok());
        self
    }

    pub fn with_training_file_id(mut self, training_file_id: Option<String>) -> Self {
        self.training_file_id = training_file_id;
        self
    }

    pub fn with_validation_file_id(mut self, validation_file_id: Option<String>) -> Self {
        self.validation_file_id = validation_file_id;
        self
    }
}

#[derive(Debug, AsChangeset, Clone, Default)]
#[diesel(table_name = finetune_jobs)]
pub struct DbUpdateFinetuneJob {
    pub state: Option<String>,
    pub fine_tuned_model: Option<Option<String>>,
    pub error_message: Option<Option<String>>,
    pub hyperparameters: Option<Option<String>>,
    pub completed_at: Option<Option<String>>,
    pub updated_at: Option<String>,
}

impl DbUpdateFinetuneJob {
    pub fn new() -> Self {
        Self {
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }

    pub fn with_state(mut self, state: FinetuneJobState) -> Self {
        self.state = Some(state.to_string());
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn with_fine_tuned_model(mut self, fine_tuned_model: Option<String>) -> Self {
        self.fine_tuned_model = Some(fine_tuned_model);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn with_error_message(mut self, error_message: Option<String>) -> Self {
        self.error_message = Some(error_message);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn with_hyperparameters(
        mut self,
        hyperparameters: Option<HashMap<String, serde_json::Value>>,
    ) -> Self {
        self.hyperparameters = Some(hyperparameters.and_then(|h| serde_json::to_string(&h).ok()));
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn with_completed_at(mut self, completed_at: Option<String>) -> Self {
        self.completed_at = Some(completed_at);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }
}
