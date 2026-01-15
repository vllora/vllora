use crate::handlers::finetune::get_langdb_api_key;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{error, info, warn};
use vllora_core::credentials::KeyStorage;
use vllora_core::events::ui_broadcaster::EventsUIBroadcaster;
use vllora_core::metadata::models::finetune_job::{
    DbFinetuneJob, DbUpdateFinetuneJob, FinetuneJobState,
};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::finetune_job::FinetuneJobService;
use vllora_core::metadata::services::project::ProjectServiceImpl;
use vllora_core::types::metadata::services::project::ProjectService;
use vllora_finetune::LangdbCloudFinetuneClient;
use vllora_llm::types::events::{CustomEventType, Event, EventRunContext};

/// State tracker for finetune jobs that periodically polls cloud API
/// and updates the local database with current job statuses
pub struct FinetuneJobStateTracker {
    db_pool: DbPool,
    key_storage: Arc<Box<dyn KeyStorage>>,
    broadcaster: Arc<EventsUIBroadcaster>,
    poll_interval: Duration,
}

impl FinetuneJobStateTracker {
    /// Create a new state tracker
    pub fn new(
        db_pool: DbPool,
        key_storage: Arc<Box<dyn KeyStorage>>,
        broadcaster: Arc<EventsUIBroadcaster>,
    ) -> Self {
        let poll_interval_secs = std::env::var("FINETUNE_STATE_TRACKER_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);

        Self {
            db_pool,
            key_storage,
            broadcaster,
            poll_interval: Duration::from_secs(poll_interval_secs),
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
                    .update_job_status(
                        &job,
                        &client,
                        &finetune_job_service,
                        project_slug.as_deref(),
                    )
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
        project_slug: Option<&str>,
    ) -> Result<(), String> {
        // Query cloud API for current status
        let status_response = client
            .get_reinforcement_job_status(&job.provider_job_id)
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

        if current_state == new_state {
            // No change, skip update
            return Ok(());
        }

        // Update database
        let mut update = DbUpdateFinetuneJob::new().with_state(new_state);

        if let Some(model_id) = status_response.fine_tuned_model {
            update = update.with_fine_tuned_model(Some(model_id));
        }

        if let Some(error) = status_response.error_message {
            update = update.with_error_message(Some(error));
        }

        // Set completed_at if job reached terminal state
        if matches!(
            new_state,
            FinetuneJobState::Succeeded | FinetuneJobState::Failed | FinetuneJobState::Cancelled
        ) {
            update = update.with_completed_at(Some(chrono::Utc::now().to_rfc3339()));
        }

        let updated_job = service
            .update(&job.id, &job.project_id, update)
            .map_err(|e| format!("Failed to update job in database: {}", e))?;

        info!(
            "Updated job {} from {:?} to {:?}",
            job.provider_job_id, current_state, new_state
        );

        // Send FinetuneJobUpdate event
        let event = Event::Custom {
            run_context: EventRunContext {
                run_id: None,
                thread_id: None,
                span_id: None,
                parent_span_id: None,
            },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            custom_event: CustomEventType::FinetuneJobUpdate {
                job_id: updated_job.id.clone(),
                status: updated_job.state.clone(),
            },
        };

        // Send event to all connected clients for this project
        self.broadcaster
            .send_events(project_slug.unwrap_or_default(), &[event])
            .await;

        Ok(())
    }
}
