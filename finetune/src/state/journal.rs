//! FileJournal: `pipeline-journal.json` reader/writer backed by the local
//! filesystem with atomic writes + advisory lock. Mirrors server-side in
//! `workflows.pipeline_journal` (TEXT) via Feature 002's gateway client.
//!
//! Track: A | Feature: 002-state-and-gateway-client
//! Schema: `finetune/src/state/schemas/journal.schema.json`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::{json, Value};

use super::atomic_write::atomic_write_string;
use super::{Journal, Result};

const JOURNAL_FILE: &str = "pipeline-journal.json";
const SCHEMA_VERSION: u32 = 1;

/// On-disk journal backed by `<project_dir>/pipeline-journal.json`. One
/// instance per `finetune-project/` directory. Takes a workflow ID at
/// construction so new journals are self-describing.
pub struct FileJournal {
    path: PathBuf,
    workflow_id: String,
}

impl FileJournal {
    /// Open an existing journal, or initialise a fresh one if the file
    /// doesn't exist. `workflow_id` is only persisted if we're initialising.
    pub fn open_or_create(project_dir: &Path, workflow_id: &str) -> Result<Self> {
        std::fs::create_dir_all(project_dir)?;
        let path = project_dir.join(JOURNAL_FILE);

        if !path.exists() {
            let now = Utc::now().to_rfc3339();
            let doc = json!({
                "schema_version": SCHEMA_VERSION,
                "workflow_id": workflow_id,
                "created_at": now,
                "updated_at": now,
                "current_phase": Value::Null,
                "phases": {},
            });
            atomic_write_string(&path, &serde_json::to_string_pretty(&doc)?)?;
        }

        Ok(Self {
            path,
            workflow_id: workflow_id.to_string(),
        })
    }

    /// Workflow ID this journal describes.
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Path to the on-disk file. Exposed for doctor / status verbs that want to cite it.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> Result<Value> {
        let raw = std::fs::read_to_string(&self.path)?;
        let mut doc: Value = serde_json::from_str(&raw)?;
        check_schema_version(&doc)?;
        // Ensure the `phases` map exists (defensive for hand-edited files).
        if doc.get("phases").is_none() {
            doc["phases"] = json!({});
        }
        Ok(doc)
    }

    fn save(&self, doc: &Value) -> Result<()> {
        atomic_write_string(&self.path, &serde_json::to_string_pretty(doc)?)
    }

    fn update<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut Value) -> Result<()>,
    {
        let mut doc = self.load()?;
        f(&mut doc)?;
        doc["updated_at"] = json!(Utc::now().to_rfc3339());
        self.save(&doc)
    }
}

impl Journal for FileJournal {
    fn read(&self) -> Result<Value> {
        self.load()
    }

    fn write_step_start(&self, step: &str, pid: u32) -> Result<()> {
        self.update(|doc| {
            let phases = phases_mut(doc);
            phases[step] = json!({
                "status": "running",
                "pid": pid,
                "started_at": Utc::now().to_rfc3339(),
            });
            doc["current_phase"] = json!(step);
            Ok(())
        })
    }

    fn write_step_done(&self, step: &str, fields: BTreeMap<String, Value>) -> Result<()> {
        self.update(|doc| {
            let phases = phases_mut(doc);
            let entry = phases
                .as_object_mut()
                .and_then(|m| m.get_mut(step))
                .ok_or_else(|| {
                    Box::<dyn std::error::Error + Send + Sync>::from(format!(
                        "step '{}' was not started — call write_step_start first",
                        step
                    ))
                })?;
            entry["status"] = json!("done");
            entry["completed_at"] = json!(Utc::now().to_rfc3339());
            if !fields.is_empty() {
                let obj = entry.as_object_mut().expect("entry is an object");
                obj.entry("fields".to_string())
                    .or_insert_with(|| json!({}));
                let f = obj["fields"].as_object_mut().expect("fields is an object");
                for (k, v) in fields {
                    f.insert(k, v);
                }
            }
            doc["current_phase"] = Value::Null;
            Ok(())
        })
    }

    fn write_step_failed(&self, step: &str, error: &str) -> Result<()> {
        self.update(|doc| {
            let phases = phases_mut(doc);
            let entry = phases
                .as_object_mut()
                .and_then(|m| m.get_mut(step))
                .ok_or_else(|| {
                    Box::<dyn std::error::Error + Send + Sync>::from(format!(
                        "step '{}' was not started — call write_step_start first",
                        step
                    ))
                })?;
            entry["status"] = json!("failed");
            entry["completed_at"] = json!(Utc::now().to_rfc3339());
            entry["error"] = json!(error);
            doc["current_phase"] = Value::Null;
            Ok(())
        })
    }

    fn write_step_iteration(
        &self,
        step: &str,
        iteration: u32,
        fields: BTreeMap<String, Value>,
    ) -> Result<()> {
        self.update(|doc| {
            let phases = phases_mut(doc);
            let entry = phases
                .as_object_mut()
                .and_then(|m| m.get_mut(step))
                .ok_or_else(|| {
                    Box::<dyn std::error::Error + Send + Sync>::from(format!(
                        "step '{}' was not started — call write_step_start first",
                        step
                    ))
                })?;
            entry["status"] = json!("iterating");
            entry["iteration"] = json!(iteration);
            if !fields.is_empty() {
                let obj = entry.as_object_mut().expect("entry is an object");
                obj.entry("fields".to_string())
                    .or_insert_with(|| json!({}));
                let f = obj["fields"].as_object_mut().expect("fields is an object");
                for (k, v) in fields {
                    f.insert(k, v);
                }
            }
            Ok(())
        })
    }

    fn is_phase_done(&self, step: &str) -> Result<bool> {
        let doc = self.load()?;
        Ok(doc
            .get("phases")
            .and_then(|p| p.get(step))
            .and_then(|e| e.get("status"))
            .and_then(|s| s.as_str())
            == Some("done"))
    }

    fn current_step(&self) -> Result<Option<String>> {
        let doc = self.load()?;
        Ok(doc
            .get("current_phase")
            .and_then(|v| v.as_str())
            .map(String::from))
    }

    fn schema_version(&self) -> u32 {
        SCHEMA_VERSION
    }
}

fn phases_mut(doc: &mut Value) -> &mut Value {
    if doc.get("phases").is_none() {
        doc["phases"] = json!({});
    }
    doc.get_mut("phases").expect("phases inserted above")
}

fn check_schema_version(doc: &Value) -> Result<()> {
    let v = doc
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            Box::<dyn std::error::Error + Send + Sync>::from(
                "journal missing schema_version field",
            )
        })? as u32;
    if v > SCHEMA_VERSION {
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
            "journal schema_version {} is newer than this binary ({}). Upgrade vllora.",
            v, SCHEMA_VERSION
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "journal-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_and_round_trip() {
        let dir = fresh_project();
        let j = FileJournal::open_or_create(&dir, "wf-001").unwrap();
        let doc = j.read().unwrap();
        assert_eq!(doc["schema_version"].as_u64(), Some(1));
        assert_eq!(doc["workflow_id"].as_str(), Some("wf-001"));
        assert!(doc["current_phase"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_then_done_sequence() {
        let dir = fresh_project();
        let j = FileJournal::open_or_create(&dir, "wf-002").unwrap();

        j.write_step_start("init", 1234).unwrap();
        let mid = j.read().unwrap();
        assert_eq!(mid["current_phase"].as_str(), Some("init"));
        assert_eq!(mid["phases"]["init"]["status"].as_str(), Some("running"));
        assert_eq!(mid["phases"]["init"]["pid"].as_u64(), Some(1234));

        let mut fields = BTreeMap::new();
        fields.insert("workflow_id".into(), json!("wf-002"));
        j.write_step_done("init", fields).unwrap();

        let final_state = j.read().unwrap();
        assert!(final_state["current_phase"].is_null());
        assert_eq!(final_state["phases"]["init"]["status"].as_str(), Some("done"));
        assert_eq!(
            final_state["phases"]["init"]["fields"]["workflow_id"].as_str(),
            Some("wf-002")
        );

        assert!(j.is_phase_done("init").unwrap());
        assert!(!j.is_phase_done("sources").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fail_records_error() {
        let dir = fresh_project();
        let j = FileJournal::open_or_create(&dir, "wf-003").unwrap();
        j.write_step_start("sources", 42).unwrap();
        j.write_step_failed("sources", "gateway 503").unwrap();
        let doc = j.read().unwrap();
        assert_eq!(doc["phases"]["sources"]["status"].as_str(), Some("failed"));
        assert_eq!(doc["phases"]["sources"]["error"].as_str(), Some("gateway 503"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn iteration_tracking() {
        let dir = fresh_project();
        let j = FileJournal::open_or_create(&dir, "wf-004").unwrap();
        j.write_step_start("eval", 99).unwrap();
        let mut f = BTreeMap::new();
        f.insert("readiness_score".into(), json!(0.42));
        j.write_step_iteration("eval", 3, f).unwrap();
        let doc = j.read().unwrap();
        assert_eq!(doc["phases"]["eval"]["status"].as_str(), Some("iterating"));
        assert_eq!(doc["phases"]["eval"]["iteration"].as_u64(), Some(3));
        assert_eq!(
            doc["phases"]["eval"]["fields"]["readiness_score"].as_f64(),
            Some(0.42)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn done_without_start_errors() {
        let dir = fresh_project();
        let j = FileJournal::open_or_create(&dir, "wf-005").unwrap();
        assert!(j.write_step_done("plan", BTreeMap::new()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_step_tracks_running() {
        let dir = fresh_project();
        let j = FileJournal::open_or_create(&dir, "wf-006").unwrap();
        assert_eq!(j.current_step().unwrap(), None);
        j.write_step_start("init", 1).unwrap();
        assert_eq!(j.current_step().unwrap(), Some("init".to_string()));
        j.write_step_done("init", BTreeMap::new()).unwrap();
        assert_eq!(j.current_step().unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_newer_schema_version() {
        let dir = fresh_project();
        let path = dir.join(JOURNAL_FILE);
        std::fs::write(
            &path,
            r#"{"schema_version": 99, "workflow_id": "wf", "created_at": "x", "updated_at": "y", "current_phase": null, "phases": {}}"#,
        )
        .unwrap();
        // Constructing doesn't re-read, but read() must reject.
        let j = FileJournal::open_or_create(&dir, "wf").unwrap();
        assert!(j.read().is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
