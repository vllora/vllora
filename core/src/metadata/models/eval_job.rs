use crate::metadata::schema::eval_jobs;
use diesel::{AsChangeset, Identifiable, Insertable, Queryable, Selectable};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Serialize, Deserialize, Queryable, Selectable, Identifiable, Clone, PartialEq, Eq,
)]
#[diesel(table_name = eval_jobs)]
#[serde(crate = "serde")]
pub struct DbEvalJob {
    pub id: String,
    pub workflow_id: String,
    pub cloud_run_id: Option<String>,
    pub status: String,
    pub sample_size: Option<i32>,
    pub rollout_model: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub started_at: Option<String>,
    pub polling_snapshot: Option<String>,
    pub result: Option<String>,
}

#[derive(Debug, Insertable, Clone)]
#[diesel(table_name = eval_jobs)]
pub struct DbNewEvalJob {
    pub id: String,
    pub workflow_id: String,
    pub cloud_run_id: Option<String>,
    pub status: String,
    pub sample_size: Option<i32>,
    pub rollout_model: Option<String>,
}

impl DbNewEvalJob {
    pub fn new(
        workflow_id: String,
        cloud_run_id: Option<String>,
        sample_size: Option<i32>,
        rollout_model: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            workflow_id,
            cloud_run_id,
            status: "pending".to_string(),
            sample_size,
            rollout_model,
        }
    }
}

#[derive(Debug, AsChangeset, Clone, Default)]
#[diesel(table_name = eval_jobs)]
pub struct DbUpdateEvalJob {
    pub cloud_run_id: Option<String>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub updated_at: Option<String>,
    pub completed_at: Option<String>,
    pub started_at: Option<String>,
    pub polling_snapshot: Option<String>,
    pub result: Option<String>,
}

impl DbUpdateEvalJob {
    pub fn with_status(status: String) -> Self {
        Self {
            status: Some(status),
            updated_at: Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            ..Default::default()
        }
    }

    pub fn with_error(status: String, error: String) -> Self {
        Self {
            status: Some(status),
            error: Some(error),
            updated_at: Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            ..Default::default()
        }
    }

    pub fn with_full_update(
        cloud_run_id: Option<String>,
        status: Option<String>,
        error: Option<String>,
        completed_at: Option<String>,
        started_at: Option<String>,
        polling_snapshot: Option<String>,
        result: Option<String>,
    ) -> Self {
        Self {
            cloud_run_id,
            status,
            error,
            updated_at: Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            completed_at,
            started_at,
            polling_snapshot,
            result,
        }
    }
}
