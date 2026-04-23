//! jobs test-job — smoke test for the job lifecycle.
//!
//! Track: A | Feature: 001+002 | Design: Feature 001 FR-018 catalog

use clap::Parser;
use serde_json::json;
use vllora_core::metadata::models::job::{DbNewJob, JobState};
use vllora_core::metadata::models::job_log::DbNewJobLog;
use vllora_core::metadata::pool::DbPool;
use vllora_core::metadata::services::project::ProjectServiceImpl;
use vllora_core::metadata::services::job::JobService;
use vllora_core::metadata::services::job_log::JobLogService;
use vllora_core::metadata::services::workflow::WorkflowService;
use vllora_core::types::metadata::services::project::ProjectService;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Idempotency key — same key + payload returns existing job_id.
    #[arg(long)]
    pub idempotency_key: Option<String>,

    /// Create job only; skip live tracking. Use `jobs status` later.
    #[arg(long)]
    pub only_tracking: bool,

    /// Arbitrary JSON input for the operation (per-op schemas TBD).
    #[arg(long)]
    pub input: Option<String>,

    /// Workflow ID to associate with this dummy job.
    #[arg(long)]
    pub workflow_id: Option<String>,

    /// Print periodic status until terminal state.
    #[arg(long, default_value_t = true)]
    pub track: bool,
}

async fn poll_until_terminal(
    job_service: &JobService,
    job_id: &str,
) -> Result<(), crate::CliError> {
    loop {
        let current = job_service
            .get_by_id(job_id)
            .map_err(|e| crate::CliError::CustomError(format!("status lookup failed: {}", e)))?
            .ok_or_else(|| crate::CliError::CustomError("job disappeared".to_string()))?;

        let progress = current
            .progress_json
            .as_ref()
            .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok());
        let percent = progress
            .as_ref()
            .and_then(|p| p.get("percent"))
            .and_then(|v| v.as_u64());
        let last_message = progress
            .as_ref()
            .and_then(|p| p.get("last_message"))
            .and_then(|v| v.as_str());

        println!(
            "{}",
            json!({
                "type": "status",
                "job_id": current.id,
                "state": current.state,
                "percent": percent,
                "last_message": last_message,
                "progress": progress
            })
        );

        if matches!(current.state.as_str(), "completed" | "failed" | "cancelled") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    Ok(())
}

pub async fn handle(db_pool: DbPool, args: Args) -> Result<(), crate::CliError> {
    let job_service = JobService::new(db_pool.clone());
    let log_service = JobLogService::new(db_pool.clone());

    let workflow_id = match args.workflow_id {
        Some(id) => id,
        None => {
            let workflow_service = WorkflowService::new(db_pool.clone());
            let workflows = workflow_service
                .list()
                .map_err(|e| crate::CliError::CustomError(format!("list workflows failed: {}", e)))?;
            let latest = workflows.into_iter().next().ok_or_else(|| {
                crate::CliError::CustomError(
                    "no workflow found; run `vllora finetune init \"<objective>\"` first or pass --workflow-id"
                        .to_string(),
                )
            })?;
            latest.id
        }
    };
    let project_service = ProjectServiceImpl::new(db_pool.clone());
    let project_id = project_service
        .get_default(uuid::Uuid::nil())
        .map(|p| p.id.to_string())
        .map_err(|e| crate::CliError::CustomError(format!("resolve default project failed: {}", e)))?;
    let idempotency_key = args
        .idempotency_key
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let fingerprint = {
        let payload = json!({
            "job_type": "test_job",
            "input": args.input,
        });
        let mut h = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        payload.to_string().hash(&mut h);
        format!("{:016x}", h.finish())
    };

    if let Some(existing) = job_service
        .get_by_idempotency(&workflow_id, &idempotency_key)
        .map_err(|e| crate::CliError::CustomError(format!("idempotency lookup failed: {}", e)))?
    {
        if existing.request_fingerprint.as_deref() != Some(fingerprint.as_str()) {
            return Err(crate::CliError::CustomError(format!(
                "idempotency_key `{}` already used with different payload",
                idempotency_key
            )));
        }
        println!(
            "{}",
            json!({
                "type": "job_created",
                "job_id": existing.id,
                "state": existing.state,
                "replay": true
            })
        );
        if !args.only_tracking && args.track {
            poll_until_terminal(&job_service, &existing.id).await?;
        }
        return Ok(());
    }

    let created = job_service
        .create(DbNewJob {
            idempotency_key: Some(idempotency_key.clone()),
            request_fingerprint: Some(fingerprint),
            ..DbNewJob::new(
                project_id,
                workflow_id,
                "test_job".to_string(),
                "test_job".to_string(),
                JobState::Queued,
            )
        })
        .map_err(|e| crate::CliError::CustomError(format!("create test job failed: {}", e)))?;

    log_service
        .create(DbNewJobLog::new(
            created.id.clone(),
            "info".to_string(),
            "queued".to_string(),
            Some(
                json!({
                    "job_type": "test_job",
                    "idempotency_key": idempotency_key
                })
                .to_string(),
            ),
        ))
        .map_err(|e| crate::CliError::CustomError(format!("log queued failed: {}", e)))?;

    println!(
        "{}",
        json!({
            "type": "job_created",
            "job_id": created.id,
            "state": created.state,
            "job_type": "test_job"
        })
    );

    if !args.only_tracking && args.track {
        poll_until_terminal(&job_service, &created.id).await?;
    }

    Ok(())
}
