use crate::handlers::finetune::get_langdb_api_key;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{error, info, warn};
use vllora_core::credentials::KeyStorage;
use vllora_core::metadata::models::finetune_job::{
    DbFinetuneJob, DbUpdateFinetuneJob, FinetuneJobState,
};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::finetune_job::FinetuneJobService;
use vllora_core::metadata::services::project::ProjectServiceImpl;
use vllora_core::metadata::services::workflow_record::WorkflowRecordScoreService;
use vllora_core::types::metadata::services::project::ProjectService;
use vllora_finetune::LangdbCloudFinetuneClient;

/// State tracker for finetune jobs that periodically polls cloud API
/// and updates the local database with current job statuses
/// Rate-limit finetune score fetches: only every N-th poll cycle per job.
const SCORE_FETCH_INTERVAL: u64 = 3;

pub struct FinetuneJobStateTracker {
    db_pool: DbPool,
    key_storage: Arc<Box<dyn KeyStorage>>,
    poll_interval: Duration,
    poll_count: AtomicU64,
}

impl FinetuneJobStateTracker {
    /// Create a new state tracker
    pub fn new(db_pool: DbPool, key_storage: Arc<Box<dyn KeyStorage>>) -> Self {
        let poll_interval_secs = std::env::var("FINETUNE_STATE_TRACKER_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);

        Self {
            db_pool,
            key_storage,
            poll_interval: Duration::from_secs(poll_interval_secs),
            poll_count: AtomicU64::new(0),
        }
    }

    /// Start the state tracker background task
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = time::interval(self.poll_interval);
            interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            info!(
                "Finetune job state tracker started with interval: {:?}",
                self.poll_interval
            );

            loop {
                interval.tick().await;

                if let Err(e) = self.poll_and_update().await {
                    error!("Error in finetune job state tracker: {}", e);
                }
            }
        })
    }

    async fn poll_and_update(&self) -> Result<(), String> {
        let finetune_job_service = FinetuneJobService::new(self.db_pool.clone());

        // Get all jobs with pending or running state
        let jobs = finetune_job_service
            .list_pending_or_running()
            .map_err(|e| format!("Failed to list pending/running jobs: {}", e))?;

        if jobs.is_empty() {
            return Ok(());
        }

        info!("Polling status for {} finetune jobs", jobs.len());

        // Group jobs by project_id to batch API key lookups
        let mut jobs_by_project: std::collections::HashMap<String, Vec<DbFinetuneJob>> =
            std::collections::HashMap::new();
        for job in jobs {
            jobs_by_project
                .entry(job.project_id.clone())
                .or_default()
                .push(job);
        }

        // Get project service to look up project slugs
        let project_service = ProjectServiceImpl::new(self.db_pool.clone());

        for (project_id, project_jobs) in jobs_by_project {
            // Get project to retrieve slug
            // Convert project_id string to UUID for lookup
            let project_uuid = match uuid::Uuid::parse_str(&project_id) {
                Ok(uuid) => uuid,
                Err(_) => {
                    warn!("Invalid project_id format: {}", project_id);
                    continue;
                }
            };

            let project_slug = project_service
                .get_by_id(project_uuid, uuid::Uuid::nil()) // owner_id not used for lookup
                .ok()
                .map(|p| p.slug);

            let api_key_result =
                get_langdb_api_key(self.key_storage.as_ref().as_ref(), project_slug.as_deref())
                    .await;

            let api_key = match api_key_result {
                Ok(key) => key,
                Err(e) => {
                    warn!("Failed to get API key for project {}: {}", project_id, e);
                    continue;
                }
            };

            let client = match LangdbCloudFinetuneClient::new(api_key) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to create client for project {}: {}", project_id, e);
                    continue;
                }
            };

            for job in project_jobs {
                if let Err(e) = self
                    .update_job_status(&job, &client, &finetune_job_service)
                    .await
                {
                    warn!(
                        "Failed to update status for job {}: {}",
                        job.provider_job_id, e
                    );
                }
            }
        }

        Ok(())
    }

    async fn update_job_status(
        &self,
        job: &DbFinetuneJob,
        client: &LangdbCloudFinetuneClient,
        service: &FinetuneJobService,
    ) -> Result<(), String> {
        // Query cloud API for current status
        let status_response = client
            .get_finetune_job_status(&job.provider_job_id)
            .await
            .map_err(|e| format!("Failed to get status from cloud API: {}", e))?;

        // Convert status string to enum
        let new_state = status_response
            .status
            .parse::<FinetuneJobState>()
            .map_err(|e| format!("Invalid status from API: {}", e))?;

        // Check if state has changed
        let current_state = job
            .state_enum()
            .map_err(|e| format!("Invalid current state: {}", e))?;

        let status_changed = current_state != new_state;

        // Update DB only when status actually changes
        if status_changed {
            let mut update = DbUpdateFinetuneJob::new().with_state(new_state);

            if let Some(model_id) = status_response.fine_tuned_model {
                update = update.with_fine_tuned_model(Some(model_id));
            }

            if let Some(error) = status_response.error_message {
                update = update.with_error_message(Some(error));
            }

            if matches!(
                new_state,
                FinetuneJobState::Succeeded
                    | FinetuneJobState::Failed
                    | FinetuneJobState::Cancelled
            ) {
                update = update.with_completed_at(Some(chrono::Utc::now().to_rfc3339()));
            }

            service
                .update(&job.id, &job.project_id, update)
                .map_err(|e| format!("Failed to update job in database: {}", e))?;

            info!(
                "Updated job {} from {:?} to {:?}",
                job.provider_job_id, current_state, new_state
            );
        }

        // Write finetune scores on every poll (rate-limited to every Nth cycle)
        let cycle = self.poll_count.fetch_add(1, Ordering::Relaxed);
        if cycle.is_multiple_of(SCORE_FETCH_INTERVAL) {
            if let Err(e) = self.write_finetune_scores(job, client).await {
                warn!(
                    "Failed to write finetune scores for job {}: {}",
                    job.provider_job_id, e
                );
            }
        }

        Ok(())
    }

    async fn write_finetune_scores(
        &self,
        job: &DbFinetuneJob,
        client: &LangdbCloudFinetuneClient,
    ) -> Result<(), String> {
        let eval_results = client
            .get_finetune_evaluations(
                &job.workflow_id,
                None,
                None,
                Some(job.provider_job_id.clone()),
                false,
                Some(10_000),
                Some(0),
            )
            .await
            .map_err(|e| format!("Failed to get finetune evaluations: {}", e))?;

        let updates: Vec<(String, f32)> = eval_results
            .results
            .iter()
            .filter_map(|row| {
                let record_id = row.row.as_ref()?.get("id")?.as_str()?.to_string();
                // Average all epoch scores for this row
                let all_scores: Vec<f64> = row
                    .epochs
                    .values()
                    .flat_map(|results| {
                        results
                            .iter()
                            .filter_map(|v| v.get("score").and_then(|s| s.as_f64()))
                    })
                    .collect();
                if all_scores.is_empty() {
                    return None;
                }
                let avg = all_scores.iter().sum::<f64>() / all_scores.len() as f64;
                Some((record_id, avg as f32))
            })
            .collect();

        if updates.is_empty() {
            return Ok(());
        }

        let score_service = WorkflowRecordScoreService::new(self.db_pool.clone());
        score_service
            .batch_upsert(&job.workflow_id, &job.provider_job_id, "finetune", &updates)
            .map_err(|e| format!("Failed to batch upsert finetune scores: {}", e))?;

        info!(
            "Wrote {} finetune scores for workflow {} (job {})",
            updates.len(),
            job.workflow_id,
            job.provider_job_id
        );

        Ok(())
    }
}
