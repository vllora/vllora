//! Atomic file writes — tmp + fsync + rename recipe.
//!
//! Track: A | Feature: 002-state-and-gateway-client
//!
//! Guarantees: after `atomic_write_string` returns, readers see either the
//! pre-state or the post-state, never a partial write. Required by Feature
//! 002 FR-001 (parent §9 invariant: state file integrity).

use std::fs::File;
use std::io::Write;
use std::path::Path;

use super::Result;

/// Write `contents` to `path` atomically. Strategy:
///   1. Write to `<path>.tmp.<pid>` in the same directory.
///   2. `fsync` the tmp file.
///   3. `rename` tmp → target (atomic on POSIX; durable on Windows).
///   4. `fsync` the containing directory (optional; best-effort on Windows).
///
/// If any step fails, the tmp file is removed and the error is returned.
/// The target path is never partially written.
pub fn atomic_write_string(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Box::<dyn std::error::Error + Send + Sync>::from(
            format!("path has no parent: {}", path.display()),
        ))?;
    std::fs::create_dir_all(parent)?;

    let tmp_name = format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file"),
        std::process::id()
    );
    let tmp_path = parent.join(tmp_name);

    // Write tmp + fsync.
    let result = (|| -> Result<()> {
        let mut tmp = File::create(&tmp_path)?;
        tmp.write_all(contents.as_bytes())?;
        tmp.sync_all()?;
        drop(tmp);
        std::fs::rename(&tmp_path, path)?;
        // Best-effort: fsync the directory so the rename survives a crash.
        // Ignored on platforms where opening a directory is not supported.
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_then_reads_back() {
        let dir = std::env::temp_dir().join(format!("atomic-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.json");
        atomic_write_string(&path, "hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrite_replaces_prior() {
        let dir = std::env::temp_dir().join(format!("atomic-overwrite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.json");
        atomic_write_string(&path, "one").unwrap();
        atomic_write_string(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn creates_parent_dir() {
        let dir = std::env::temp_dir().join(format!("atomic-parent-{}", std::process::id()));
        let path = dir.join("nested").join("subdir").join("file.json");
        atomic_write_string(&path, "x").unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
