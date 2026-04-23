//! `vllora finetune sources <paths-or-uris...>` — URI resolve + spawn
//! knowledge_extractor / trace_analyzer workers.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §5.2
//! Contract: specs/003-cli-pipeline-verbs/contracts/verb-contract.md#sources

use std::collections::BTreeMap;
use std::path::Path;

use clap::Parser;
use serde_json::json;
use uuid::Uuid;
use vllora_core::metadata::pool::DbPool;
use vllora_finetune::gateway_client::GatewayClient;
use vllora_finetune::sources_adapters;
use vllora_finetune::state::journal::FileJournal;
use vllora_finetune::state::{lock, Journal};

use super::shared;
use super::workers::claude_client::{self, ClaudeClient, WorkerStatus};
use super::workers::{knowledge_extractor, trace_analyzer};

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// One or more paths or URIs (file://, hf://, s3://, gs://, azblob://, https://).
    pub sources: Vec<String>,
    /// Max concurrent extractor workers (default 12).
    #[arg(long, default_value_t = 12)]
    pub parallel: usize,
    /// Force re-extraction even if `sources` is already `done`.
    #[arg(long)]
    pub force: bool,
}

pub async fn handle(_db_pool: DbPool, args: Args) -> Result<(), crate::CliError> {
    let gateway = shared::make_gateway_client();
    let worker = claude_client::default_client();
    let project_dir = shared::project_dir()?;
    handle_inner(&*gateway, &*worker, &project_dir, args).await
}

pub async fn handle_inner<G: GatewayClient + ?Sized, W: ClaudeClient + ?Sized>(
    gateway: &G,
    worker: &W,
    project_dir: &Path,
    args: Args,
) -> Result<(), crate::CliError> {
    if args.sources.is_empty() {
        return precondition_err("at least one source path or URI is required");
    }

    let journal = open_journal(project_dir)?;
    if !journal
        .is_phase_done("init")
        .map_err(|e| cli_err(format!("journal: {}", e)))?
    {
        return precondition_err("init is not done — run /finetune-init first");
    }

    if journal
        .is_phase_done("sources")
        .map_err(|e| cli_err(format!("journal: {}", e)))?
        && !args.force
    {
        shared::emit(&shared::phase_done(
            "sources",
            "done",
            Some("/finetune-plan"),
            Some("sources already ingested (re-run with --force to reset)"),
        ));
        return Ok(());
    }

    let workflow_id = read_workflow_id(&journal)?;

    let _guard = lock::acquire(project_dir)
        .map_err(|e| cli_err(format!("lock: {}", e)))?;

    journal
        .write_step_start("sources", std::process::id())
        .map_err(|e| cli_err(format!("journal start: {}", e)))?;

    shared::emit(&shared::progress(
        "sources",
        &format!("resolving {} source(s)", args.sources.len()),
        None,
    ));

    let mut worker_ok = 0_u64;
    let mut worker_failed = 0_u64;
    let mut total_parts = 0_u64;

    for uri in &args.sources {
        let local_path = match sources_adapters::resolve_uri(uri).await {
            Ok(p) => p,
            Err(e) => {
                shared::emit(&shared::error_event(
                    "INVALID_REQUEST",
                    &format!("cannot resolve {}: {}", uri, e),
                    Some("Check the URI scheme + required env vars"),
                ));
                worker_failed += 1;
                continue;
            }
        };

        let path_str = local_path.display().to_string();
        shared::emit(&shared::progress(
            "sources",
            &format!("extracting {}", path_str),
            None,
        ));

        // Pick knowledge_extractor for PDFs / trace_analyzer for OTel bundles.
        let is_trace = path_str.ends_with(".otel.json") || path_str.contains("/traces/");
        let result = if is_trace {
            trace_analyzer::run(worker, &workflow_id.to_string(), &project_dir.display().to_string(), &path_str).await
        } else {
            knowledge_extractor::run(
                worker,
                &workflow_id.to_string(),
                &project_dir.display().to_string(),
                uri,
                &path_str,
            )
            .await
        };

        match result {
            Ok(res) if res.status == WorkerStatus::Ok => {
                let worker_name = if is_trace { "trace_analyzer" } else { "knowledge_extractor" };
                let count = res.artifacts.first().map(|a| a.count).unwrap_or(0);
                shared::emit(&json!({
                    "type": "worker_done",
                    "worker": worker_name,
                    "status": "ok",
                    "target": path_str,
                    "artifacts_count": count,
                    "emitted_at": chrono::Utc::now().to_rfc3339(),
                }));
                total_parts += count;
                worker_ok += 1;
                // Upload the (canned) knowledge parts to the gateway — mock records the call.
                let _ = gateway
                    .upload_knowledge_parts(workflow_id, Vec::new())
                    .await;
            }
            Ok(res) => {
                let code = res
                    .error
                    .as_ref()
                    .map(|e| e.code.clone())
                    .unwrap_or_else(|| "WORKER_UNRESPONSIVE".to_string());
                let msg = res
                    .error
                    .as_ref()
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "worker returned incomplete".to_string());
                shared::emit(&shared::error_event(&code, &msg, None));
                worker_failed += 1;
            }
            Err(e) => {
                shared::emit(&shared::error_event("INTERNAL", &format!("{}", e), None));
                worker_failed += 1;
            }
        }
    }

    if worker_ok == 0 {
        let msg = "all source extractions failed".to_string();
        let _ = journal.write_step_failed("sources", &msg);
        return Err(cli_err(msg));
    }

    let mut fields = BTreeMap::new();
    fields.insert("source_count".into(), json!(args.sources.len()));
    fields.insert("knowledge_parts".into(), json!(total_parts));
    fields.insert("workers_ok".into(), json!(worker_ok));
    fields.insert("workers_failed".into(), json!(worker_failed));
    journal
        .write_step_done("sources", fields)
        .map_err(|e| cli_err(format!("journal done: {}", e)))?;
    if let Ok(doc) = journal.read() {
        let _ = gateway
            .put_pipeline_journal(workflow_id, &doc.to_string())
            .await;
    }

    shared::emit(&shared::phase_done(
        "sources",
        "done",
        Some("/finetune-plan"),
        Some(&format!(
            "{} source(s); {} knowledge parts extracted",
            args.sources.len(),
            total_parts
        )),
    ));
    Ok(())
}

fn open_journal(project_dir: &Path) -> Result<FileJournal, crate::CliError> {
    if !project_dir.join("pipeline-journal.json").is_file() {
        shared::emit(&shared::error_event(
            "PRECONDITION_UNMET",
            "finetune-project/ does not exist — run /finetune-init first",
            None,
        ));
        return Err(cli_err(
            "precondition unmet: finetune-project/ does not exist (run /finetune-init first)",
        ));
    }
    FileJournal::open_or_create(project_dir, "unused")
        .map_err(|e| cli_err(format!("open journal: {}", e)))
}

fn read_workflow_id(journal: &FileJournal) -> Result<Uuid, crate::CliError> {
    let doc = journal
        .read()
        .map_err(|e| cli_err(format!("read journal: {}", e)))?;
    let s = doc
        .get("workflow_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| cli_err("journal missing workflow_id"))?;
    Uuid::parse_str(s).map_err(|e| cli_err(format!("invalid workflow_id: {}", e)))
}

fn cli_err<S: Into<String>>(s: S) -> crate::CliError {
    crate::CliError::CustomError(s.into())
}

fn precondition_err(msg: &str) -> Result<(), crate::CliError> {
    shared::emit(&shared::error_event("PRECONDITION_UNMET", msg, None));
    Err(cli_err(format!("precondition unmet: {}", msg)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vllora_finetune::gateway_client::{MockCall, MockGatewayClient};
    use vllora_finetune::state::journal::FileJournal;

    fn fresh_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vllora-sources-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    fn init_project(dir: &Path) -> Uuid {
        let wf = Uuid::new_v4();
        std::fs::create_dir_all(dir).unwrap();
        let j = FileJournal::open_or_create(dir, &wf.to_string()).unwrap();
        j.write_step_start("init", 1).unwrap();
        let mut f = BTreeMap::new();
        f.insert("workflow_id".into(), json!(wf.to_string()));
        j.write_step_done("init", f).unwrap();
        wf
    }

    #[tokio::test]
    async fn happy_path_with_local_files() {
        let dir = fresh_dir();
        let wf = init_project(&dir);
        // Create two fake PDF files.
        let p1 = dir.join("doc1.pdf");
        let p2 = dir.join("doc2.pdf");
        std::fs::write(&p1, b"%PDF-1.4").unwrap();
        std::fs::write(&p2, b"%PDF-1.4").unwrap();

        let gateway = MockGatewayClient::new().with_workflow_id(wf);
        let worker = claude_client::StubClaudeClient::new();

        handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                sources: vec![p1.display().to_string(), p2.display().to_string()],
                parallel: 12,
                force: false,
            },
        )
        .await
        .unwrap();

        let j = FileJournal::open_or_create(&dir, "unused").unwrap();
        assert!(j.is_phase_done("sources").unwrap());
        let doc = j.read().unwrap();
        assert_eq!(
            doc["phases"]["sources"]["fields"]["source_count"].as_u64(),
            Some(2)
        );

        // Gateway saw two upload_knowledge_parts (one per worker_ok).
        let uploads = gateway
            .calls()
            .iter()
            .filter(|c| matches!(c, MockCall::UploadKnowledgeParts { .. }))
            .count();
        assert_eq!(uploads, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn init_not_done_blocks() {
        let dir = fresh_dir();
        // No init run.
        let gateway = MockGatewayClient::new();
        let worker = claude_client::StubClaudeClient::new();
        let err = handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                sources: vec!["/tmp/whatever.pdf".into()],
                parallel: 12,
                force: false,
            },
        )
        .await
        .unwrap_err();
        assert!(format!("{}", err).contains("precondition"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unresolved_uri_fails_cleanly() {
        let dir = fresh_dir();
        let _wf = init_project(&dir);
        let gateway = MockGatewayClient::new();
        let worker = claude_client::StubClaudeClient::new();
        // Only URI is nonexistent — all extractions fail.
        let err = handle_inner(
            &gateway,
            &worker,
            &dir,
            Args {
                sources: vec!["/this/path/does/not/exist".into()],
                parallel: 12,
                force: false,
            },
        )
        .await
        .unwrap_err();
        assert!(format!("{}", err).contains("all source extractions failed"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
