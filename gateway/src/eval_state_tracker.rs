use crate::handlers::finetune::get_langdb_api_key;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{error, info, warn};
use vllora_core::credentials::KeyStorage;
use vllora_core::events::ui_broadcaster::EventsUIBroadcaster;
use vllora_core::metadata::models::eval_job::{DbEvalJob, DbUpdateEvalJob};
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::eval_job::EvalJobService;
use vllora_core::metadata::services::project::ProjectServiceImpl;
use vllora_core::metadata::services::workflow_record::WorkflowRecordScoreService;
use vllora_core::types::metadata::services::project::ProjectService;
use vllora_finetune::LangdbCloudFinetuneClient;
use vllora_finetune::types::RowEpochResults;
use vllora_llm::types::events::{CustomEventType, Event, EventRunContext};

const TERMINAL_STATUSES: &[&str] = &["completed", "failed", "cancelled"];

/// State tracker for eval jobs that periodically polls the cloud API
/// and updates the local database with current job statuses
pub struct EvalJobStateTracker {
    db_pool: DbPool,
    key_storage: Arc<Box<dyn KeyStorage>>,
    broadcaster: Arc<EventsUIBroadcaster>,
    poll_interval: Duration,
}

impl EvalJobStateTracker {
    pub fn new(
        db_pool: DbPool,
        key_storage: Arc<Box<dyn KeyStorage>>,
        broadcaster: Arc<EventsUIBroadcaster>,
    ) -> Self {
        let poll_interval_secs = std::env::var("EVAL_STATE_TRACKER_INTERVAL_SECS")
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
                "Eval job state tracker started with interval: {:?}",
                self.poll_interval
            );

            loop {
                interval.tick().await;

                if let Err(e) = self.poll_and_update().await {
                    error!("Error in eval job state tracker: {}", e);
                }
            }
        })
    }

    async fn poll_and_update(&self) -> Result<(), String> {
        let eval_job_service = EvalJobService::new(self.db_pool.clone());

        // Get all jobs with pending or running status
        let pending_jobs = eval_job_service
            .list_by_status("pending")
            .map_err(|e| format!("Failed to list pending eval jobs: {}", e))?;

        let running_jobs = eval_job_service
            .list_by_status("running")
            .map_err(|e| format!("Failed to list running eval jobs: {}", e))?;

        let jobs: Vec<DbEvalJob> = pending_jobs
            .into_iter()
            .chain(running_jobs)
            .filter(|job| job.cloud_run_id.is_some())
            .collect();

        if jobs.is_empty() {
            return Ok(());
        }

        info!("Polling status for {} eval jobs", jobs.len());

        // Eval jobs don't have a project_id, so use the default project's API key
        let project_service = ProjectServiceImpl::new(self.db_pool.clone());
        let project_slug = project_service
            .get_default(uuid::Uuid::nil())
            .ok()
            .map(|p| p.slug);

        let api_key =
            get_langdb_api_key(self.key_storage.as_ref().as_ref(), project_slug.as_deref())
                .await
                .map_err(|e| format!("Failed to get API key for eval state tracker: {}", e))?;

        let client = LangdbCloudFinetuneClient::new(api_key)
            .map_err(|e| format!("Failed to create finetune client: {}", e))?;

        for job in &jobs {
            if let Err(e) = self
                .update_job_status(job, &client, &eval_job_service)
                .await
            {
                let cloud_id = job.cloud_run_id.as_deref().unwrap_or("unknown");
                warn!(
                    "Failed to update status for eval job {} (cloud: {}): {}",
                    job.id, cloud_id, e
                );
            }
        }

        Ok(())
    }

    async fn update_job_status(
        &self,
        job: &DbEvalJob,
        client: &LangdbCloudFinetuneClient,
        service: &EvalJobService,
    ) -> Result<(), String> {
        let cloud_run_id = job
            .cloud_run_id
            .as_deref()
            .ok_or("Job has no cloud_run_id")?;

        // Query cloud API for current status
        let result_response = client
            .get_evaluation_result(cloud_run_id)
            .await
            .map_err(|e| format!("Failed to get eval result from cloud API: {}", e))?;

        let new_status = &result_response.status;
        let status_changed = job.status != *new_status;

        // Always save the polling_snapshot so FE has fresh progress data
        let snapshot_json = serde_json::to_string(&result_response)
            .map_err(|e| format!("Failed to serialize polling snapshot: {}", e))?;

        // Build update changeset: always include snapshot, conditionally include status
        if status_changed {
            let mut update = DbUpdateEvalJob {
                status: Some(new_status.clone()),
                polling_snapshot: Some(snapshot_json),
                updated_at: Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                ..Default::default()
            };

            if new_status == "failed" {
                update.error = Some(format!(
                    "Evaluation failed (completed: {}, failed: {}, total: {})",
                    result_response.completed_rows,
                    result_response.failed_rows,
                    result_response.total_rows,
                ));
            }

            if TERMINAL_STATUSES.contains(&new_status.as_str()) {
                update.completed_at =
                    Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
            }

            service
                .update_full(&job.id, update)
                .map_err(|e| format!("Failed to update eval job in database: {}", e))?;

            info!(
                "Updated eval job {} (cloud: {}) from {:?} to {:?}",
                job.id, cloud_run_id, job.status, new_status
            );

            let update_event = Event::Custom {
                run_context: EventRunContext::default(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                custom_event: CustomEventType::EvalJobUpdate {
                    job_id: job.id.clone(),
                    workflow_id: job.workflow_id.clone(),
                    status: new_status.clone(),
                },
            };

            let broadcaster = self.broadcaster.clone();
            let events = vec![update_event];
            tokio::spawn(async move {
                broadcaster.send_events("", &events).await;
            });

            if TERMINAL_STATUSES.contains(&new_status.as_str()) {
                info!("Eval job {} reached terminal status: {}", job.id, new_status);
            }
        } else {
            // Status unchanged, just update the snapshot for progress tracking
            service
                .update_full(
                    &job.id,
                    DbUpdateEvalJob {
                        polling_snapshot: Some(snapshot_json),
                        updated_at: Some(
                            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                        ),
                        ..Default::default()
                    },
                )
                .map_err(|e| format!("Failed to update eval job snapshot: {}", e))?;
        }

        // Write eval scores on every poll (incremental updates while running)
        if !result_response.results.is_empty() {
            self.write_eval_scores(&job.workflow_id, cloud_run_id, &result_response.results)?;
        }

        Ok(())
    }

    fn write_eval_scores(
        &self,
        workflow_id: &str,
        job_id: &str,
        results: &[RowEpochResults],
    ) -> Result<(), String> {
        let updates: Vec<(String, f32)> = results
            .iter()
            .filter_map(|row| {
                let record_id = extract_record_id(&row.row)?;
                let score = extract_eval_score(&row.epochs)?;
                Some((record_id, score))
            })
            .collect();

        if updates.is_empty() {
            return Ok(());
        }

        let score_service = WorkflowRecordScoreService::new(self.db_pool.clone());
        score_service
            .batch_upsert(workflow_id, job_id, "eval", &updates)
            .map_err(|e| format!("Failed to batch upsert eval scores: {}", e))?;

        info!(
            "Wrote {} eval scores for workflow {}",
            updates.len(),
            workflow_id
        );

        // Broadcast SSE event
        let event = Event::Custom {
            run_context: EventRunContext::default(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            custom_event: CustomEventType::RecordScoresUpdated {
                workflow_id: workflow_id.to_string(),
                score_type: "eval".to_string(),
                updated_count: updates.len(),
            },
        };

        let broadcaster = self.broadcaster.clone();
        tokio::spawn(async move {
            broadcaster.send_events("", &[event]).await;
        });

        Ok(())
    }
}

fn extract_record_id(row: &Option<serde_json::Value>) -> Option<String> {
    row.as_ref()?.get("id")?.as_str().map(|s| s.to_string())
}

fn extract_eval_score(
    epochs: &std::collections::HashMap<String, Vec<serde_json::Value>>,
) -> Option<f32> {
    // Use the latest epoch's average score
    let max_epoch_key = epochs.keys().filter_map(|k| k.parse::<i32>().ok()).max()?;
    let epoch_results = epochs.get(&max_epoch_key.to_string())?;

    let scores: Vec<f64> = epoch_results
        .iter()
        .filter_map(|v| v.get("score").and_then(|s| s.as_f64()))
        .collect();

    if scores.is_empty() {
        return None;
    }

    let avg = scores.iter().sum::<f64>() / scores.len() as f64;
    Some(avg as f32)
}
