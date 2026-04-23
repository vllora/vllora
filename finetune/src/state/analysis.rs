//! FileAnalysis — append-only `analysis.json` running-diary backed by the
//! local filesystem with atomic writes. Mirrors server-side in
//! `workflows.iteration_state` (TEXT) via `GatewayClient::put_iteration_state`.
//!
//! Track: A | Feature: 002-state-and-gateway-client
//! Schema: `finetune/src/state/schemas/analysis.schema.json`
//!
//! Invariants (enforced by the API — there's no overwrite primitive):
//!   - Once a phase section exists, its `reasoning` / `decisions` /
//!     `iterations` / `artifacts` are byte-preserved.
//!   - Corrections flow through `augmentations[]`, never by rewriting prior content.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::{json, Value};

use super::atomic_write::atomic_write_string;
use super::{Analysis, Result};

const ANALYSIS_FILE: &str = "analysis.json";
const SCHEMA_VERSION: u32 = 1;

pub struct FileAnalysis {
    path: PathBuf,
    workflow_id: String,
}

impl FileAnalysis {
    /// Open or create the `analysis.json` under `project_dir`.
    pub fn open_or_create(project_dir: &Path, workflow_id: &str) -> Result<Self> {
        std::fs::create_dir_all(project_dir)?;
        let path = project_dir.join(ANALYSIS_FILE);
        if !path.exists() {
            let doc = json!({
                "schema_version": SCHEMA_VERSION,
                "workflow_id": workflow_id,
                "phases": {},
            });
            atomic_write_string(&path, &serde_json::to_string_pretty(&doc)?)?;
        }
        Ok(Self {
            path,
            workflow_id: workflow_id.to_string(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    fn load(&self) -> Result<Value> {
        let raw = std::fs::read_to_string(&self.path)?;
        let doc: Value = serde_json::from_str(&raw)?;
        check_schema_version(&doc)?;
        Ok(doc)
    }

    fn save(&self, doc: &Value) -> Result<()> {
        atomic_write_string(&self.path, &serde_json::to_string_pretty(doc)?)
    }
}

impl Analysis for FileAnalysis {
    /// Create a new phase entry. Fails fast if the phase already exists — use
    /// `augment_phase` for corrections after the fact.
    fn append_phase(&self, phase: &str, content: Value) -> Result<()> {
        let mut doc = self.load()?;
        if !doc.get("phases").is_some_and(|v| v.is_object()) {
            doc["phases"] = json!({});
        }
        let phases = doc["phases"].as_object_mut().expect("phases is object");
        if phases.contains_key(phase) {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "phase '{}' already exists — use augment_phase for corrections",
                phase
            )));
        }
        let mut entry = content;
        entry["written_at"] = json!(Utc::now().to_rfc3339());
        phases.insert(phase.into(), entry);
        self.save(&doc)
    }

    /// Append a correction note to an existing phase. Preserves prior content.
    fn augment_phase(&self, phase: &str, additions: Value) -> Result<()> {
        let mut doc = self.load()?;
        let entry = doc["phases"]
            .as_object_mut()
            .and_then(|m| m.get_mut(phase))
            .ok_or_else(|| {
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "phase '{}' has not been created yet — call append_phase first",
                    phase
                ))
            })?;
        let obj = entry.as_object_mut().ok_or_else(|| {
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "phase '{}' entry is not an object — corrupt analysis.json",
                phase
            ))
        })?;
        obj.entry("augmentations".to_string())
            .or_insert_with(|| json!([]));
        let arr = obj["augmentations"]
            .as_array_mut()
            .expect("augmentations is array");
        let mut note = match additions {
            Value::Object(_) => additions,
            other => json!({ "note": other }),
        };
        note.as_object_mut()
            .expect("note is object")
            .insert("written_at".into(), json!(Utc::now().to_rfc3339()));
        arr.push(note);
        self.save(&doc)
    }

    fn read_phase(&self, phase: &str) -> Result<Option<Value>> {
        let doc = self.load()?;
        Ok(doc.get("phases").and_then(|p| p.get(phase)).cloned())
    }

    fn read_full(&self) -> Result<Value> {
        self.load()
    }
}

fn check_schema_version(doc: &Value) -> Result<()> {
    let v = doc
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            Box::<dyn std::error::Error + Send + Sync>::from(
                "analysis missing schema_version field",
            )
        })? as u32;
    if v > SCHEMA_VERSION {
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
            "analysis schema_version {} is newer than this binary ({}). Upgrade vllora.",
            v, SCHEMA_VERSION
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> PathBuf {
        std::env::temp_dir().join(format!(
            "analysis-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    #[test]
    fn create_and_read_empty() {
        let dir = fresh();
        let a = FileAnalysis::open_or_create(&dir, "wf").unwrap();
        let doc = a.read_full().unwrap();
        assert_eq!(doc["schema_version"].as_u64(), Some(1));
        assert!(doc["phases"].as_object().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_then_read_phase() {
        let dir = fresh();
        let a = FileAnalysis::open_or_create(&dir, "wf").unwrap();
        a.append_phase(
            "plan",
            json!({
                "reasoning": "topics designed by domain, leaf granularity ~20 records",
                "decisions": [{"label": "granularity", "choice": "20", "rationale": "GRPO sweet spot"}],
            }),
        )
        .unwrap();
        let p = a.read_phase("plan").unwrap().unwrap();
        assert_eq!(
            p["reasoning"].as_str(),
            Some("topics designed by domain, leaf granularity ~20 records")
        );
        assert!(p["written_at"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn double_append_same_phase_errors() {
        let dir = fresh();
        let a = FileAnalysis::open_or_create(&dir, "wf").unwrap();
        a.append_phase("plan", json!({})).unwrap();
        assert!(a.append_phase("plan", json!({})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn augment_preserves_prior() {
        let dir = fresh();
        let a = FileAnalysis::open_or_create(&dir, "wf").unwrap();
        a.append_phase("plan", json!({"reasoning": "initial"}))
            .unwrap();
        a.augment_phase("plan", json!({"note": "clarification"}))
            .unwrap();
        let p = a.read_phase("plan").unwrap().unwrap();
        assert_eq!(p["reasoning"].as_str(), Some("initial"));
        assert_eq!(
            p["augmentations"][0]["note"].as_str(),
            Some("clarification")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn augment_without_append_errors() {
        let dir = fresh();
        let a = FileAnalysis::open_or_create(&dir, "wf").unwrap();
        assert!(a.augment_phase("plan", json!({"note": "x"})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
