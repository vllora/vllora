//! `vllora finetune import-dataset <path> [--schema X]` — alternative entry
//! path that ingests pre-built records from JSONL / Parquet. Skips sources +
//! plan + generate.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs
//! Contract: specs/003-cli-pipeline-verbs/contracts/verb-contract.md#import-dataset
//!
//! MVP scope: local JSONL only (schema validation is minimal — one record per
//! line, each parseable as JSON). HF / S3 / Parquet / strict schema checking
//! are future work.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde_json::{json, Value};
use uuid::Uuid;
use vllora_core::metadata::pool::DbPool;
use vllora_finetune::gateway_client::{GatewayClient, Record as UploadRecord};
use vllora_finetune::state::journal::FileJournal;
use vllora_finetune::state::{lock, Journal};

use super::shared;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Local path to the records file (JSONL). URIs (hf://, s3://, …)
    /// land in a later iteration.
    pub path: PathBuf,
    /// Schema hint (`openai-chat` | `tool-calling`). Informational in MVP.
    #[arg(long)]
    pub schema: Option<String>,
    /// Reset + re-import even if `import-dataset` already marked done.
    #[arg(long)]
    pub force: bool,
}

pub async fn handle(_db_pool: DbPool, args: Args) -> Result<(), crate::CliError> {
    let gateway = shared::make_gateway_client();
    let project_dir = shared::project_dir()?;
    handle_inner(&*gateway, &project_dir, args).await
}

pub async fn handle_inner<G: GatewayClient + ?Sized>(
    gateway: &G,
    project_dir: &Path,
    args: Args,
) -> Result<(), crate::CliError> {
    // 1. init must be done; the gen-path phases must NOT be done (mutual exclusion).
    let journal_path = project_dir.join("pipeline-journal.json");
    if !journal_path.is_file() {
        shared::emit(&shared::error_event(
            "PRECONDITION_UNMET",
            "finetune-project/ does not exist — run /finetune-init first",
            Some("vllora finetune init \"<objective>\""),
        ));
        return Err(crate::CliError::CustomError(
            "precondition unmet: init not run".into(),
        ));
    }
    let journal = FileJournal::open_or_create(project_dir, "unknown")
        .map_err(|e| crate::CliError::CustomError(format!("open journal: {}", e)))?;

    let is_done = |phase: &str| {
        journal
            .is_phase_done(phase)
            .map_err(|e| crate::CliError::CustomError(format!("journal read {}: {}", phase, e)))
    };

    if !is_done("init")? {
        return precondition_error("init is not done — run /finetune-init first");
    }
    for gen_phase in ["sources", "plan", "generate"] {
        if is_done(gen_phase)? {
            return precondition_error(&format!(
                "{} is already done — import-dataset is mutually exclusive with the generation path",
                gen_phase
            ));
        }
    }
    if is_done("import-dataset")? && !args.force {
        shared::emit(&shared::phase_done(
            "import-dataset",
            "done",
            Some("/finetune-eval"),
            Some("import-dataset already complete (re-run with --force to reset)"),
        ));
        return Ok(());
    }

    // 2. Single-writer lock.
    let _guard = lock::acquire(project_dir)
        .map_err(|e| crate::CliError::CustomError(format!("lock: {}", e)))?;

    // 3. Validate + parse the file.
    shared::emit(&shared::progress(
        "import-dataset",
        &format!("validating {}", args.path.display()),
        None,
    ));

    let workflow_id = parse_workflow_id(&journal)?;
    let records = match load_and_validate(&args.path, workflow_id, args.schema.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            shared::emit(&shared::error_event("INVALID_REQUEST", &e, None));
            journal
                .write_step_start("import-dataset", std::process::id())
                .ok();
            journal
                .write_step_failed("import-dataset", &e)
                .ok();
            return Err(crate::CliError::CustomError(e));
        }
    };

    let count = records.len();
    shared::emit(&shared::progress(
        "import-dataset",
        &format!("uploading {} records", count),
        None,
    ));

    // 4. Open the import-dataset phase, upload, close.
    journal
        .write_step_start("import-dataset", std::process::id())
        .map_err(|e| crate::CliError::CustomError(format!("journal start: {}", e)))?;

    gateway
        .upload_records(workflow_id, records)
        .await
        .map_err(|e| crate::CliError::CustomError(format!("gateway.upload_records: {}", e)))?;

    let mut fields = BTreeMap::new();
    fields.insert("record_count".into(), json!(count));
    fields.insert(
        "origin_uri".into(),
        json!(format!("file://{}", args.path.display())),
    );
    if let Some(s) = &args.schema {
        fields.insert("schema".into(), json!(s));
    }
    journal
        .write_step_done("import-dataset", fields)
        .map_err(|e| crate::CliError::CustomError(format!("journal done: {}", e)))?;

    if let Ok(doc) = journal.read() {
        let _ = gateway
            .put_pipeline_journal(workflow_id, &doc.to_string())
            .await;
    }

    shared::emit(&shared::phase_done(
        "import-dataset",
        "done",
        Some("/finetune-eval"),
        Some(&format!("{} records imported", count)),
    ));
    Ok(())
}

fn precondition_error(msg: &str) -> Result<(), crate::CliError> {
    shared::emit(&shared::error_event("PRECONDITION_UNMET", msg, None));
    Err(crate::CliError::CustomError(format!(
        "precondition unmet: {}",
        msg
    )))
}

/// Pull the workflow ID out of the journal. Stored at root + in `phases.init.fields.workflow_id`.
fn parse_workflow_id(journal: &FileJournal) -> Result<Uuid, crate::CliError> {
    let doc = journal
        .read()
        .map_err(|e| crate::CliError::CustomError(format!("read journal: {}", e)))?;
    let id_str = doc
        .get("workflow_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            crate::CliError::CustomError("journal missing workflow_id".into())
        })?;
    Uuid::parse_str(id_str)
        .map_err(|e| crate::CliError::CustomError(format!("invalid workflow_id: {}", e)))
}

fn load_and_validate(
    path: &Path,
    workflow_id: Uuid,
    schema_hint: Option<&str>,
) -> Result<Vec<UploadRecord>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let origin_uri = format!("file://{}", path.display());
    let mut records = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)
            .map_err(|e| format!("line {}: not JSON — {}", lineno + 1, e))?;
        // Minimal schema check: `messages` must be a non-empty array.
        let messages = v
            .get("messages")
            .and_then(|m| m.as_array())
            .ok_or_else(|| {
                format!("line {}: missing or non-array `messages` field", lineno + 1)
            })?;
        if messages.is_empty() {
            return Err(format!("line {}: `messages` is empty", lineno + 1));
        }
        let ground_truth = v.get("ground_truth").cloned().unwrap_or(Value::Null);
        let tools = v.get("tools").cloned();

        // Per-record topic_id: ad-hoc UUID derived from an index; Track A
        // will replace this with the gateway-assigned topic ID once topics
        // are wired.
        records.push(UploadRecord {
            topic_id: Uuid::new_v4(),
            origin_uri: Some(origin_uri.clone()),
            origin_source_id: None,
            messages_json: serde_json::to_string(messages).expect("messages serialises"),
            ground_truth_json: ground_truth.to_string(),
            tools_json: tools.map(|t| t.to_string()),
        });
    }

    if records.is_empty() {
        return Err(format!("{} contained no records", path.display()));
    }
    let _ = (workflow_id, schema_hint); // validated elsewhere; here they're informational
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use vllora_finetune::gateway_client::{MockCall, MockGatewayClient};

    fn fresh_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vllora-imp-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    fn init_journal(dir: &Path) -> Uuid {
        let fixed = Uuid::new_v4();
        std::fs::create_dir_all(dir).unwrap();
        let j = FileJournal::open_or_create(dir, &fixed.to_string()).unwrap();
        j.write_step_start("init", 1).unwrap();
        let mut f = BTreeMap::new();
        f.insert("workflow_id".into(), json!(fixed.to_string()));
        j.write_step_done("init", f).unwrap();
        fixed
    }

    fn write_jsonl(path: &Path, n: usize) {
        let mut lines = Vec::new();
        for i in 0..n {
            lines.push(format!(
                r#"{{"messages":[{{"role":"user","content":"hi {}"}}],"ground_truth":{{"answer":"hello {}"}}}}"#,
                i, i
            ));
        }
        std::fs::write(path, lines.join("\n")).unwrap();
    }

    #[tokio::test]
    async fn happy_path_imports_local_jsonl() {
        let dir = fresh_dir();
        let wf = init_journal(&dir);
        let records_path = dir.join("records.jsonl");
        write_jsonl(&records_path, 3);

        let gateway = MockGatewayClient::new();
        let args = Args {
            path: records_path.clone(),
            schema: Some("openai-chat".into()),
            force: false,
        };
        handle_inner(&gateway, &dir, args).await.unwrap();

        // Mock saw an upload_records with the right workflow + count.
        let calls = gateway.calls();
        let upload = calls
            .iter()
            .find(|c| matches!(c, MockCall::UploadRecords { .. }))
            .expect("upload_records not recorded");
        match upload {
            MockCall::UploadRecords { workflow_id, count } => {
                assert_eq!(*workflow_id, wf);
                assert_eq!(*count, 3);
            }
            _ => unreachable!(),
        }

        // Journal reflects done + record_count.
        let j = FileJournal::open_or_create(&dir, "unused").unwrap();
        let doc = j.read().unwrap();
        assert_eq!(
            doc["phases"]["import-dataset"]["status"].as_str(),
            Some("done")
        );
        assert_eq!(
            doc["phases"]["import-dataset"]["fields"]["record_count"].as_u64(),
            Some(3)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn missing_project_dir_errors_precondition() {
        let dir = fresh_dir();
        let gateway = MockGatewayClient::new();
        let args = Args {
            path: "/tmp/nowhere.jsonl".into(),
            schema: None,
            force: false,
        };
        let err = handle_inner(&gateway, &dir, args).await.unwrap_err();
        assert!(format!("{}", err).to_lowercase().contains("precondition"));
    }

    #[tokio::test]
    async fn sources_done_blocks_import() {
        let dir = fresh_dir();
        let _wf = init_journal(&dir);
        // Mark sources done.
        let j = FileJournal::open_or_create(&dir, "unused").unwrap();
        j.write_step_start("sources", 1).unwrap();
        j.write_step_done("sources", BTreeMap::new()).unwrap();

        let records_path = dir.join("records.jsonl");
        write_jsonl(&records_path, 1);

        let gateway = MockGatewayClient::new();
        let args = Args {
            path: records_path,
            schema: None,
            force: false,
        };
        let err = handle_inner(&gateway, &dir, args).await.unwrap_err();
        assert!(format!("{}", err).contains("mutually exclusive"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn invalid_jsonl_surfaces_line_number() {
        let dir = fresh_dir();
        let _wf = init_journal(&dir);
        let records_path = dir.join("bad.jsonl");
        // Line 1 is valid, line 2 is malformed.
        std::fs::write(
            &records_path,
            "{\"messages\":[{\"role\":\"user\",\"content\":\"x\"}]}\n\
             this-is-not-json\n",
        )
        .unwrap();

        let gateway = MockGatewayClient::new();
        let args = Args {
            path: records_path,
            schema: None,
            force: false,
        };
        let err = handle_inner(&gateway, &dir, args).await.unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("line 2"), "expected 'line 2' in err: {}", msg);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
