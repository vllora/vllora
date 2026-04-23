use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{error, info, warn};
use vllora_core::metadata::models::job::{DbJob, DbUpdateJob, JobState};
use vllora_core::metadata::models::job_log::DbNewJobLog;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::job::JobService;
use vllora_core::metadata::services::job_log::JobLogService;

const DEFAULT_POLL_INTERVAL_SECS: u64 = 2;
const DEFAULT_BATCH_SIZE: i64 = 10;

pub struct JobQueueRunner {
    db_pool: DbPool,
    poll_interval: Duration,
    batch_size: i64,
    executors: Arc<HashMap<String, Arc<dyn JobExecutor>>>,
}

impl JobQueueRunner {
    pub fn new(db_pool: DbPool) -> Self {
        let poll_interval_secs = std::env::var("JOB_QUEUE_RUNNER_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);

        let batch_size = std::env::var("JOB_QUEUE_RUNNER_BATCH_SIZE")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_BATCH_SIZE);

        let mut executors: HashMap<String, Arc<dyn JobExecutor>> = HashMap::new();
        let test_job_executor: Arc<dyn JobExecutor> = Arc::new(TestJobExecutor);
        executors.insert(test_job_executor.job_type().to_string(), test_job_executor);

        Self {
            db_pool,
            poll_interval: Duration::from_secs(poll_interval_secs),
            batch_size,
            executors: Arc::new(executors),
        }
    }

    pub fn enabled() -> bool {
        std::env::var("JOB_QUEUE_RUNNER_ENABLED")
            .map(|v| !matches!(v.as_str(), "0" | "false" | "FALSE" | "no" | "NO"))
            .unwrap_or(true)
    }

    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = time::interval(self.poll_interval);
            interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            info!(
                "Job queue runner started (interval: {:?}, batch_size: {})",
                self.poll_interval, self.batch_size
            );

            loop {
                interval.tick().await;
                if let Err(e) = self.process_once().await {
                    error!("Job queue runner tick failed: {}", e);
                }
            }
        })
    }

    async fn process_once(&self) -> Result<(), String> {
        let service = JobService::new(self.db_pool.clone());
        let queued = service
            .list_by_state("queued", self.batch_size)
            .map_err(|e| format!("failed to list queued jobs: {}", e))?;

        if queued.is_empty() {
            return Ok(());
        }

        for job in queued {
            let claimed = service
                .try_claim_queued(&job.id)
                .map_err(|e| format!("failed to claim queued job {}: {}", job.id, e))?;
            if !claimed {
                continue;
            }

            let db_pool = self.db_pool.clone();
            let executors = self.executors.clone();
            tokio::spawn(async move {
                if let Err(e) = execute_job(db_pool, executors, job).await {
                    error!("Job execution failed: {}", e);
                }
            });
        }

        Ok(())
    }
}

#[async_trait]
pub trait JobExecutor: Send + Sync {
    fn job_type(&self) -> &'static str;
    async fn execute(&self, job: &DbJob, ctx: &JobExecutionContext) -> Result<(), String>;
}

pub struct JobExecutionContext {
    service: JobService,
    log_service: JobLogService,
}

impl JobExecutionContext {
    fn new(db_pool: DbPool) -> Self {
        Self {
            service: JobService::new(db_pool.clone()),
            log_service: JobLogService::new(db_pool),
        }
    }

    fn log(&self, job_id: &str, level: &str, event: &str, payload: Option<serde_json::Value>) -> Result<(), String> {
        create_log(&self.log_service, job_id, level, event, payload)
    }

    fn set_progress(&self, job_id: &str, percent: u64, message: &str) -> Result<(), String> {
        set_progress(&self.service, job_id, percent, message)
    }

    fn complete(&self, job_id: &str, result_ref: String) -> Result<(), String> {
        let completed = DbUpdateJob::new()
            .with_state(JobState::Completed)
            .with_progress_json(Some(
                json!({
                    "percent": 100,
                    "last_message": "completed"
                })
                .to_string(),
            ))
            .with_result_ref(Some(result_ref.clone()))
            .with_finished_at(Some(chrono::Utc::now().to_rfc3339()));
        self.service
            .update(job_id, completed)
            .map_err(|e| format!("failed to complete job {}: {}", job_id, e))?;
        self.log(
            job_id,
            "info",
            "completed",
            Some(json!({ "result_ref": result_ref })),
        )?;
        Ok(())
    }

    fn fail_unsupported_job_type(&self, job_id: &str, job_type: &str) -> Result<(), String> {
        let failed = DbUpdateJob::new()
            .with_state(JobState::Failed)
            .with_error(
                Some("unsupported_job_type".to_string()),
                Some(format!("Unsupported job_type: {}", job_type)),
            )
            .with_finished_at(Some(chrono::Utc::now().to_rfc3339()));
        self.service
            .update(job_id, failed)
            .map_err(|e| format!("failed to mark job {} as failed: {}", job_id, e))?;
        self.log(
            job_id,
            "error",
            "failed",
            Some(json!({
                "error_code": "unsupported_job_type",
                "error_message": format!("Unsupported job_type: {}", job_type)
            })),
        )?;
        Ok(())
    }
}

pub struct TestJobExecutor;

#[async_trait]
impl JobExecutor for TestJobExecutor {
    fn job_type(&self) -> &'static str {
        "test_job"
    }

    async fn execute(&self, job: &DbJob, ctx: &JobExecutionContext) -> Result<(), String> {
        ctx.set_progress(&job.id, 10, "starting execution")?;
        ctx.log(
            &job.id,
            "info",
            "progress",
            Some(json!({ "percent": 10, "message": "starting execution" })),
        )?;

        tokio::time::sleep(Duration::from_millis(400)).await;

        ctx.set_progress(&job.id, 65, "processing")?;
        ctx.log(
            &job.id,
            "info",
            "progress",
            Some(json!({ "percent": 65, "message": "processing" })),
        )?;

        tokio::time::sleep(Duration::from_millis(400)).await;

        ctx.complete(&job.id, format!("job://{}/result", job.id))?;
        info!("Job {} completed", job.id);
        Ok(())
    }
}

async fn execute_job(
    db_pool: DbPool,
    executors: Arc<HashMap<String, Arc<dyn JobExecutor>>>,
    job: DbJob,
) -> Result<(), String> {
    let ctx = JobExecutionContext::new(db_pool);

    ctx.log(&job.id, "info", "started", None)?;

    if let Some(executor) = executors.get(job.job_type.as_str()) {
        executor.execute(&job, &ctx).await?;
    } else {
        warn!("Unsupported job_type `{}` for job {}", job.job_type, job.id);
        ctx.fail_unsupported_job_type(&job.id, &job.job_type)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_executor_is_registered() {
        let db_pool = vllora_core::metadata::pool::establish_connection(
            ":memory:".to_string(),
            1,
        );
        vllora_core::metadata::utils::init_db(&db_pool);

        let runner = JobQueueRunner::new(db_pool);
        assert!(runner.executors.contains_key("test_job"));
    }
}

fn set_progress(service: &JobService, job_id: &str, percent: u64, message: &str) -> Result<(), String> {
    let update = DbUpdateJob::new().with_progress_json(Some(
        json!({
            "percent": percent,
            "last_message": message
        })
        .to_string(),
    ));
    service
        .update(job_id, update)
        .map_err(|e| format!("failed to update progress for job {}: {}", job_id, e))?;
    Ok(())
}

fn create_log(
    log_service: &JobLogService,
    job_id: &str,
    level: &str,
    event: &str,
    payload: Option<serde_json::Value>,
) -> Result<(), String> {
    let payload_json = payload.map(|p| p.to_string());
    log_service
        .create(DbNewJobLog::new(
            job_id.to_string(),
            level.to_string(),
            event.to_string(),
            payload_json,
        ))
        .map_err(|e| format!("failed to write job log for {}: {}", job_id, e))?;
    Ok(())
}
