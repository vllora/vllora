//! Pipeline integration tests — walk `init → status → import-dataset`
//! end-to-end with a `MockGatewayClient`. Feature 003 MVP scope.
//!
//! These tests exercise the real verb logic, real `FileJournal`, real atomic
//! writes + lock acquisition, real stream-JSON emission — only the gateway is
//! mocked. When Track A's `LangdbGatewayClient` adapter lands, production
//! via `shared::make_gateway_client()` targets the live gateway; these tests
//! stay pinned to the mock for hermetic, fast runs.
//!
//! Located inside the binary crate (rather than `gateway/tests/`) because
//! Rust binary crates don't expose their modules to external integration
//! tests — the verbs' `handle_inner` entry points are only reachable from
//! `#[cfg(test)]` modules inside the crate.

#![cfg(test)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::workers::claude_client::StubClaudeClient;
use super::{auto, eval, generate, import_dataset, init, plan, quickstart, sources, status, train};
use vllora_finetune::gateway_client::{MockCall, MockGatewayClient};
use vllora_finetune::state::journal::FileJournal;
use vllora_finetune::state::Journal;

fn scratch_project() -> PathBuf {
    std::env::temp_dir().join(format!(
        "vllora-pipeline-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ))
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

/// Scratch-dir cleanup guard.
struct DirCleanup(PathBuf);
impl Drop for DirCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn full_import_path() {
    let dir = scratch_project();
    let _cleanup = DirCleanup(dir.clone());

    let workflow_id = Uuid::new_v4();
    let gateway = MockGatewayClient::new().with_workflow_id(workflow_id);

    // 1. init
    init::handle_inner(
        &gateway,
        &dir,
        init::Args {
            objective: "build a retail support agent".into(),
            base_model: "qwen-3.5-2b".into(),
            name: Some("retail".into()),
            force: false,
        },
    )
    .await
    .expect("init should succeed");

    let journal = FileJournal::open_or_create(&dir, &workflow_id.to_string()).unwrap();
    assert!(journal.is_phase_done("init").unwrap());
    let doc = journal.read().unwrap();
    assert_eq!(
        doc["workflow_id"].as_str(),
        Some(workflow_id.to_string().as_str())
    );

    // 2. status
    status::handle_inner(&dir, status::Args {}).await.expect("status");

    // 3. import-dataset
    let records_path = dir.join("records.jsonl");
    write_jsonl(&records_path, 5);
    import_dataset::handle_inner(
        &gateway,
        &dir,
        import_dataset::Args {
            path: records_path.clone(),
            schema: Some("openai-chat".into()),
            force: false,
        },
    )
    .await
    .expect("import-dataset should succeed");

    let journal = FileJournal::open_or_create(&dir, &workflow_id.to_string()).unwrap();
    assert!(journal.is_phase_done("init").unwrap());
    assert!(journal.is_phase_done("import-dataset").unwrap());
    let doc = journal.read().unwrap();
    assert_eq!(
        doc["phases"]["import-dataset"]["fields"]["record_count"].as_u64(),
        Some(5)
    );

    let calls = gateway.calls();
    assert!(matches!(calls[0], MockCall::CreateWorkflow { .. }));
    assert!(calls.iter().any(|c| matches!(
        c,
        MockCall::UploadRecords { workflow_id: w, count } if *w == workflow_id && *count == 5
    )));
    assert!(calls
        .iter()
        .any(|c| matches!(c, MockCall::PutPipelineJournal { .. })));
}

#[tokio::test]
async fn status_before_init_suggests_init() {
    let dir = scratch_project();
    let _cleanup = DirCleanup(dir.clone());
    status::handle_inner(&dir, status::Args {}).await.unwrap();
}

#[tokio::test]
async fn cannot_import_after_sources_done() {
    let dir = scratch_project();
    let _cleanup = DirCleanup(dir.clone());

    let gateway = MockGatewayClient::new();
    init::handle_inner(
        &gateway,
        &dir,
        init::Args {
            objective: "x".into(),
            base_model: "qwen-3.5-2b".into(),
            name: None,
            force: false,
        },
    )
    .await
    .unwrap();

    let j = FileJournal::open_or_create(&dir, "ignored").unwrap();
    j.write_step_start("sources", 1).unwrap();
    j.write_step_done("sources", BTreeMap::new()).unwrap();

    let records_path = dir.join("records.jsonl");
    write_jsonl(&records_path, 1);

    let err = import_dataset::handle_inner(
        &gateway,
        &dir,
        import_dataset::Args {
            path: records_path,
            schema: None,
            force: false,
        },
    )
    .await
    .unwrap_err();

    let msg = format!("{}", err);
    assert!(
        msg.contains("mutually exclusive"),
        "expected mutual-exclusion error, got: {}",
        msg
    );
}

#[tokio::test]
async fn rerun_init_without_force_is_noop() {
    let dir = scratch_project();
    let _cleanup = DirCleanup(dir.clone());

    let workflow_id = Uuid::new_v4();
    let gateway = MockGatewayClient::new().with_workflow_id(workflow_id);

    let args = init::Args {
        objective: "x".into(),
        base_model: "qwen-3.5-2b".into(),
        name: None,
        force: false,
    };
    init::handle_inner(&gateway, &dir, args.clone()).await.unwrap();

    let journal_before =
        std::fs::read_to_string(dir.join("pipeline-journal.json")).unwrap();
    init::handle_inner(&gateway, &dir, args).await.unwrap();
    let journal_after =
        std::fs::read_to_string(dir.join("pipeline-journal.json")).unwrap();
    assert_eq!(
        journal_before, journal_after,
        "re-running init without --force must not touch the journal"
    );
}

/// The happy-path end-to-end pipeline: init → sources → plan → generate →
/// eval (pass) → train. Every verb runs against the mock gateway +
/// stub worker. Proves the whole Track B surface composes without cross-verb
/// regressions.
#[tokio::test]
async fn full_pipeline_init_to_train() {
    let dir = scratch_project();
    let _cleanup = DirCleanup(dir.clone());

    let workflow_id = Uuid::new_v4();
    let gateway = MockGatewayClient::new().with_workflow_id(workflow_id);
    let worker = StubClaudeClient::new();

    // init
    init::handle_inner(
        &gateway,
        &dir,
        init::Args {
            objective: "build an agent".into(),
            base_model: "qwen-3.5-2b".into(),
            name: None,
            force: false,
        },
    )
    .await
    .unwrap();

    // sources — two local PDF stubs.
    let p1 = dir.join("doc1.pdf");
    let p2 = dir.join("doc2.pdf");
    std::fs::write(&p1, b"%PDF-1.4").unwrap();
    std::fs::write(&p2, b"%PDF-1.4").unwrap();
    sources::handle_inner(
        &gateway,
        &worker,
        &dir,
        sources::Args {
            sources: vec![p1.display().to_string(), p2.display().to_string()],
            parallel: 12,
            force: false,
        },
    )
    .await
    .unwrap();

    // plan
    plan::handle_inner(&gateway, &worker, &dir, plan::Args { force: false })
        .await
        .unwrap();

    // generate
    generate::handle_inner(
        &gateway,
        &worker,
        &dir,
        generate::Args {
            topics: 3,
            per_topic: 5,
            force: false,
        },
    )
    .await
    .unwrap();

    // eval
    eval::handle_inner(
        &gateway,
        &worker,
        &dir,
        eval::Args {
            max_iterations: 3,
            model: "qwen-3.5-4b".into(),
            force: false,
        },
    )
    .await
    .unwrap();

    // train
    train::handle_inner(
        &gateway,
        &worker,
        &dir,
        train::Args {
            model: "qwen-3.5-4b".into(),
            config: None,
            force: false,
        },
    )
    .await
    .unwrap();

    // Every phase done.
    let j = FileJournal::open_or_create(&dir, "unused").unwrap();
    for phase in ["init", "sources", "plan", "generate", "eval", "train"] {
        assert!(
            j.is_phase_done(phase).unwrap(),
            "phase {} should be done",
            phase
        );
    }
    let doc = j.read().unwrap();
    assert_eq!(
        doc["phases"]["eval"]["fields"]["readiness"].as_str(),
        Some("pass")
    );
    assert!(doc["phases"]["train"]["fields"]["adapter_id"]
        .as_str()
        .unwrap()
        .starts_with("adapter-"));

    // Gateway saw the expected calls — workflow creation, multiple uploads,
    // eval run creation, training job creation.
    let calls = gateway.calls();
    assert!(calls
        .iter()
        .any(|c| matches!(c, MockCall::CreateWorkflow { .. })));
    assert!(calls
        .iter()
        .any(|c| matches!(c, MockCall::UploadKnowledgeParts { .. })));
    assert!(calls
        .iter()
        .any(|c| matches!(c, MockCall::UploadTopics { .. })));
    assert!(calls
        .iter()
        .any(|c| matches!(c, MockCall::UploadRecords { .. })));
    assert!(calls
        .iter()
        .any(|c| matches!(c, MockCall::UploadGrader { .. })));
    assert!(calls
        .iter()
        .any(|c| matches!(c, MockCall::CreateEvalRun { .. })));
    assert!(calls
        .iter()
        .any(|c| matches!(c, MockCall::CreateTrainingJob { .. })));
}

/// The import-dataset path: init → import-dataset → eval → train.
#[tokio::test]
async fn full_pipeline_import_dataset_path() {
    let dir = scratch_project();
    let _cleanup = DirCleanup(dir.clone());

    let workflow_id = Uuid::new_v4();
    let gateway = MockGatewayClient::new().with_workflow_id(workflow_id);
    let worker = StubClaudeClient::new();

    init::handle_inner(
        &gateway,
        &dir,
        init::Args {
            objective: "x".into(),
            base_model: "qwen-3.5-2b".into(),
            name: None,
            force: false,
        },
    )
    .await
    .unwrap();

    let records_path = dir.join("records.jsonl");
    std::fs::write(
        &records_path,
        "{\"messages\":[{\"role\":\"user\",\"content\":\"x\"}]}\n",
    )
    .unwrap();
    import_dataset::handle_inner(
        &gateway,
        &dir,
        import_dataset::Args {
            path: records_path,
            schema: None,
            force: false,
        },
    )
    .await
    .unwrap();

    eval::handle_inner(
        &gateway,
        &worker,
        &dir,
        eval::Args {
            max_iterations: 3,
            model: "qwen-3.5-4b".into(),
            force: false,
        },
    )
    .await
    .unwrap();

    train::handle_inner(
        &gateway,
        &worker,
        &dir,
        train::Args {
            model: "qwen-3.5-4b".into(),
            config: None,
            force: false,
        },
    )
    .await
    .unwrap();

    let j = FileJournal::open_or_create(&dir, "unused").unwrap();
    assert!(j.is_phase_done("import-dataset").unwrap());
    assert!(j.is_phase_done("eval").unwrap());
    assert!(j.is_phase_done("train").unwrap());
    assert!(!j.is_phase_done("sources").unwrap());
    assert!(!j.is_phase_done("plan").unwrap());
    assert!(!j.is_phase_done("generate").unwrap());
}

/// Quickstart chains init + sources.
#[tokio::test]
async fn quickstart_chains_init_and_sources() {
    let dir = scratch_project();
    let _cleanup = DirCleanup(dir.clone());

    let wf = Uuid::new_v4();
    let gateway = MockGatewayClient::new().with_workflow_id(wf);
    let worker = StubClaudeClient::new();

    std::fs::create_dir_all(&dir).unwrap();
    let pdf = dir.join("x.pdf");
    std::fs::write(&pdf, b"%PDF-1.4").unwrap();

    quickstart::handle_inner(
        &gateway,
        &worker,
        &dir,
        quickstart::Args {
            objective: "build an agent".into(),
            sources: vec![pdf.display().to_string()],
            base_model: "qwen-3.5-2b".into(),
            name: None,
        },
    )
    .await
    .unwrap();

    let j = FileJournal::open_or_create(&dir, "unused").unwrap();
    assert!(j.is_phase_done("init").unwrap());
    assert!(j.is_phase_done("sources").unwrap());
}

/// `auto` loops through plan → generate → eval without manual invocation.
/// Stops at train (not auto-invokable without `--allow-train`).
#[tokio::test]
async fn auto_loops_plan_to_eval() {
    let dir = scratch_project();
    let _cleanup = DirCleanup(dir.clone());

    let wf = Uuid::new_v4();
    let gateway = MockGatewayClient::new().with_workflow_id(wf);
    let worker = StubClaudeClient::new();

    // Bootstrap: init + sources.
    init::handle_inner(
        &gateway,
        &dir,
        init::Args {
            objective: "x".into(),
            base_model: "qwen-3.5-2b".into(),
            name: None,
            force: false,
        },
    )
    .await
    .unwrap();
    let pdf = dir.join("x.pdf");
    std::fs::write(&pdf, b"%PDF-1.4").unwrap();
    sources::handle_inner(
        &gateway,
        &worker,
        &dir,
        sources::Args {
            sources: vec![pdf.display().to_string()],
            parallel: 12,
            force: false,
        },
    )
    .await
    .unwrap();

    // Auto should handle plan → generate → eval, then stop at train.
    auto::handle_inner(
        &gateway,
        &worker,
        &dir,
        auto::Args {
            max_iterations: 10,
            allow_train: false,
        },
    )
    .await
    .unwrap();

    let j = FileJournal::open_or_create(&dir, "unused").unwrap();
    assert!(j.is_phase_done("plan").unwrap());
    assert!(j.is_phase_done("generate").unwrap());
    assert!(j.is_phase_done("eval").unwrap());
    assert!(!j.is_phase_done("train").unwrap(), "train is deliberately manual without --allow-train");
}
