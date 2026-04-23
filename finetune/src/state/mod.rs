//! Local state file helpers for vllora finetune.
//!
//! Track: A | Feature: 002-state-and-gateway-client
//! Design: parent §4.7 + §9 invariants
//!
//! Writes local artifacts in finetune-project/ AND syncs to gateway:
//!   pipeline-journal.json  ↔  workflows.pipeline_journal (TEXT)
//!   analysis.json          ↔  workflows.iteration_state  (TEXT)
//!   change-log.md          local-only (grader audit trail)
//!   execution-log.md       local-only (per-step decision cards)
//!
//! Every write: atomic (tmp + fsync + rename), single-writer (fs2 lock),
//! schema-validated, append-only for history sections.

pub mod atomic_write;
pub mod journal;
pub mod analysis;
pub mod change_log;
pub mod execution_log;
pub mod lock;

use std::path::Path;
use std::collections::BTreeMap;
use serde_json::Value;

/// Placeholder error alias used by the skeleton. Track A picks the final
/// error type when implementing the traits (likely a dedicated `StateError`
/// enum via `thiserror`, once the concrete failure modes are known).
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

pub trait Journal {
    fn read(&self) -> Result<Value>;
    fn write_step_start(&self, step: &str, pid: u32) -> Result<()>;
    fn write_step_done(&self, step: &str, fields: BTreeMap<String, Value>) -> Result<()>;
    fn write_step_failed(&self, step: &str, error: &str) -> Result<()>;
    fn write_step_iteration(&self, step: &str, iteration: u32, fields: BTreeMap<String, Value>) -> Result<()>;
    fn is_phase_done(&self, step: &str) -> Result<bool>;
    fn current_step(&self) -> Result<Option<String>>;
    fn schema_version(&self) -> u32;
}

pub trait Analysis {
    fn append_phase(&self, phase: &str, content: Value) -> Result<()>;
    fn augment_phase(&self, phase: &str, additions: Value) -> Result<()>;
    fn read_phase(&self, phase: &str) -> Result<Option<Value>>;
    fn read_full(&self) -> Result<Value>;
}

pub trait ChangeLog {
    fn append(&self, author: &str, rationale: &str, diff: &str) -> Result<()>;
}

pub trait ExecutionLog {
    fn append(&self, observation: &str, analysis: &str, decision: &str, evidence: &str) -> Result<()>;
}

// TODO [A]: constructor functions that open journal/analysis with gateway-sync wiring.
pub fn open_journal(_project_dir: &Path, _workflow_id: &str) -> Result<impl Journal> {
    struct TodoJournal;
    impl Journal for TodoJournal {
        fn read(&self) -> Result<Value> { unimplemented!() }
        fn write_step_start(&self, _: &str, _: u32) -> Result<()> { unimplemented!() }
        fn write_step_done(&self, _: &str, _: BTreeMap<String, Value>) -> Result<()> { unimplemented!() }
        fn write_step_failed(&self, _: &str, _: &str) -> Result<()> { unimplemented!() }
        fn write_step_iteration(&self, _: &str, _: u32, _: BTreeMap<String, Value>) -> Result<()> { unimplemented!() }
        fn is_phase_done(&self, _: &str) -> Result<bool> { unimplemented!() }
        fn current_step(&self) -> Result<Option<String>> { unimplemented!() }
        fn schema_version(&self) -> u32 { 1 }
    }
    Ok(TodoJournal)
}
