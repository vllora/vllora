//! FileChangeLog — append-only `change-log.md`. Pairs with the `graders` DB
//! table's `change_reason` column to provide a full audit trail of grader
//! modifications across iterations.
//!
//! Track: A | Feature: 002-state-and-gateway-client
//!
//! Entry format (one Markdown section per append):
//!
//! ```text
//! ## 2026-04-23T09:45:12+00:00 — grader_drafter:refine
//!
//! **Rationale**: eval iter 3 showed score_concentration 0.78; raised tpFloor.
//!
//! <unified diff fenced as ```diff ... ```>
//! ```
//!
//! No overwrite primitive — `append` is the only write path. Readers can
//! stream the file or the caller can grep it.

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;

use super::{ChangeLog, Result};

const CHANGE_LOG_FILE: &str = "change-log.md";

pub struct FileChangeLog {
    path: PathBuf,
}

impl FileChangeLog {
    pub fn open_or_create(project_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(project_dir)?;
        let path = project_dir.join(CHANGE_LOG_FILE);
        if !path.exists() {
            std::fs::write(&path, "# Grader change log\n\n")?;
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

impl ChangeLog for FileChangeLog {
    fn append(&self, author: &str, rationale: &str, diff: &str) -> Result<()> {
        if rationale.trim().is_empty() {
            return Err(Box::<dyn std::error::Error + Send + Sync>::from(
                "rationale must be non-empty",
            ));
        }

        let ts = Utc::now().to_rfc3339();
        let mut entry = String::with_capacity(rationale.len() + diff.len() + 128);
        entry.push_str(&format!("## {} — {}\n\n", ts, author));
        entry.push_str("**Rationale**: ");
        entry.push_str(rationale.trim());
        entry.push_str("\n\n");
        if !diff.trim().is_empty() {
            entry.push_str("```diff\n");
            entry.push_str(diff.trim_end());
            entry.push_str("\n```\n\n");
        }

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
            "change-log-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    #[test]
    fn append_adds_entry() {
        let dir = fresh();
        let c = FileChangeLog::open_or_create(&dir).unwrap();
        c.append("human", "initial grader", "").unwrap();
        let body = c.read().unwrap();
        assert!(body.starts_with("# Grader change log"));
        assert!(body.contains("— human"));
        assert!(body.contains("initial grader"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_with_diff_includes_fenced_block() {
        let dir = fresh();
        let c = FileChangeLog::open_or_create(&dir).unwrap();
        c.append(
            "agent:grader_drafter:refine",
            "raise tpFloor to 0.22",
            "-   tpFloor = 0.02\n+   tpFloor = 0.22",
        )
        .unwrap();
        let body = c.read().unwrap();
        assert!(body.contains("```diff"));
        assert!(body.contains("tpFloor = 0.22"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_rationale_errors() {
        let dir = fresh();
        let c = FileChangeLog::open_or_create(&dir).unwrap();
        assert!(c.append("human", "   ", "diff").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multiple_appends_preserve_order() {
        let dir = fresh();
        let c = FileChangeLog::open_or_create(&dir).unwrap();
        c.append("human", "first", "").unwrap();
        c.append("human", "second", "").unwrap();
        c.append("human", "third", "").unwrap();
        let body = c.read().unwrap();
        let pos_first = body.find("first").unwrap();
        let pos_second = body.find("second").unwrap();
        let pos_third = body.find("third").unwrap();
        assert!(pos_first < pos_second && pos_second < pos_third);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
