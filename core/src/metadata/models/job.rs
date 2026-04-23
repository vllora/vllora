use crate::metadata::schema::jobs;
use diesel::{AsChangeset, Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobState::Queued => write!(f, "queued"),
            JobState::Running => write!(f, "running"),
            JobState::Completed => write!(f, "completed"),
            JobState::Failed => write!(f, "failed"),
            JobState::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = jobs)]
#[serde(crate = "serde")]
pub struct DbJob {
    pub id: String,
    pub project_id: String,
    pub workflow_id: String,
    pub job_type: String,
    pub operation: String,
    pub state: String,
    pub idempotency_key: Option<String>,
    pub request_fingerprint: Option<String>,
    pub progress_json: Option<String>,
    pub result_ref: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = jobs)]
pub struct DbNewJob {
    pub id: String,
    pub project_id: String,
    pub workflow_id: String,
    pub job_type: String,
    pub operation: String,
    pub state: String,
    pub idempotency_key: Option<String>,
    pub request_fingerprint: Option<String>,
    pub progress_json: Option<String>,
    pub result_ref: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

impl DbNewJob {
    pub fn new(
        project_id: String,
        workflow_id: String,
        job_type: String,
        operation: String,
        state: JobState,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            project_id,
            workflow_id,
            job_type,
            operation,
            state: state.to_string(),
            idempotency_key: None,
            request_fingerprint: None,
            progress_json: None,
            result_ref: None,
            error_code: None,
            error_message: None,
        }
    }
}

#[derive(Debug, AsChangeset, Clone, Default)]
#[diesel(table_name = jobs)]
pub struct DbUpdateJob {
    pub state: Option<String>,
    pub progress_json: Option<Option<String>>,
    pub result_ref: Option<Option<String>>,
    pub error_code: Option<Option<String>>,
    pub error_message: Option<Option<String>>,
    pub updated_at: Option<String>,
    pub started_at: Option<Option<String>>,
    pub finished_at: Option<Option<String>>,
}

impl DbUpdateJob {
    pub fn new() -> Self {
        Self {
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }

    pub fn with_state(mut self, state: JobState) -> Self {
        self.state = Some(state.to_string());
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn with_progress_json(mut self, progress_json: Option<String>) -> Self {
        self.progress_json = Some(progress_json);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn with_result_ref(mut self, result_ref: Option<String>) -> Self {
        self.result_ref = Some(result_ref);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn with_error(mut self, code: Option<String>, message: Option<String>) -> Self {
        self.error_code = Some(code);
        self.error_message = Some(message);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn with_started_at(mut self, started_at: Option<String>) -> Self {
        self.started_at = Some(started_at);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    pub fn with_finished_at(mut self, finished_at: Option<String>) -> Self {
        self.finished_at = Some(finished_at);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        self
    }
}
