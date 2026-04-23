//! FileExecutionLog — append-only `execution-log.md` of decision cards.
//!
//! Track: A | Feature: 002-state-and-gateway-client
//!
//! Each entry captures a CLI-level decision point with four fields:
//! observation (what we saw), analysis (how we interpreted it), decision
//! (what we did), evidence (where the supporting data lives). The verb /
//! orchestrator appends as it makes non-trivial judgment calls.
//!
//! Entry format:
//!
//! ```markdown
//! ## 2026-04-23T09:45:12+00:00
//!
//! **Observation**: …
//! **Analysis**:   …
//! **Decision**:   …
//! **Evidence**:   …
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;

use super::{ExecutionLog, Result};

const EXECUTION_LOG_FILE: &str = "execution-log.md";

pub struct FileExecutionLog {
    path: PathBuf,
}

impl FileExecutionLog {
    pub fn open_or_create(project_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(project_dir)?;
        let path = project_dir.join(EXECUTION_LOG_FILE);
        if !path.exists() {
            std::fs::write(&path, "# Execution log\n\n")?;
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self) -> Result<String> {
        Ok(std::fs::read_to_string(&self.path)?)
    }
}

impl ExecutionLog for FileExecutionLog {
    fn append(
        &self,
        observation: &str,
        analysis: &str,
        decision: &str,
        evidence: &str,
    ) -> Result<()> {
        for (field, label) in [
            (observation, "observation"),
            (analysis, "analysis"),
            (decision, "decision"),
            (evidence, "evidence"),
        ] {
            if field.trim().is_empty() {
                return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "{} must be non-empty",
                    label
                )));
            }
        }

        let ts = Utc::now().to_rfc3339();
        let entry = format!(
            "## {}\n\n**Observation**: {}\n**Analysis**: {}\n**Decision**: {}\n**Evidence**: {}\n\n",
            ts,
            observation.trim(),
            analysis.trim(),
            decision.trim(),
            evidence.trim(),
        );

        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(entry.as_bytes())?;
        f.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> PathBuf {
        std::env::temp_dir().join(format!(
            "execution-log-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    #[test]
    fn append_adds_structured_entry() {
        let dir = fresh();
        let e = FileExecutionLog::open_or_create(&dir).unwrap();
        e.append(
            "eval iter 3 score_concentration = 0.78",
            "grader is clustering scores — suspect TP-tiered floor needs adjustment",
            "raise tpFloor from 0.02 to 0.22",
            "analysis.json phases.eval.iterations[2]",
        )
        .unwrap();
        let body = e.read().unwrap();
        assert!(body.contains("**Observation**"));
        assert!(body.contains("score_concentration = 0.78"));
        assert!(body.contains("tpFloor from 0.02 to 0.22"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_field_errors() {
        let dir = fresh();
        let e = FileExecutionLog::open_or_create(&dir).unwrap();
        assert!(e.append("obs", "", "decision", "evidence").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn order_preserved() {
        let dir = fresh();
        let e = FileExecutionLog::open_or_create(&dir).unwrap();
        e.append("first", "a", "d", "e").unwrap();
        e.append("second", "a", "d", "e").unwrap();
        let body = e.read().unwrap();
        assert!(body.find("first").unwrap() < body.find("second").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
