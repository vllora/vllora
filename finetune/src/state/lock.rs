//! Single-writer advisory lock for `finetune-project/` directories.
//!
//! Track: A | Feature: 002-state-and-gateway-client
//!
//! Uses a PID file (`.vllora.lock`) to enforce that at most one `vllora`
//! process writes to a given project directory at a time. Released when
//! the `LockGuard` drops. Dead-PID recovery: a lock referencing a
//! no-longer-alive PID is reclaimed (with a warning on stderr).
//!
//! Feature 002 FR-002.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::Result;

const LOCK_FILE: &str = ".vllora.lock";

/// Acquire an advisory lock on `project_dir`. Returns a guard that releases
/// the lock on drop.
pub fn acquire(project_dir: &Path) -> Result<LockGuard> {
    if !project_dir.is_dir() {
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
            "project directory does not exist: {}",
            project_dir.display()
        )));
    }

    let lock_path = project_dir.join(LOCK_FILE);

    if lock_path.exists() {
        match read_pid(&lock_path) {
            Ok(pid) if pid == std::process::id() => {
                return Ok(LockGuard {
                    path: lock_path,
                    own_pid: true,
                });
            }
            Ok(pid) if is_process_alive(pid) => {
                return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "project locked by PID {} (lock file: {})",
                    pid,
                    lock_path.display()
                )));
            }
            _ => {
                eprintln!(
                    "Warning: reclaiming stale lock from dead PID at {}",
                    lock_path.display()
                );
                let _ = std::fs::remove_file(&lock_path);
            }
        }
    }

    let pid = std::process::id();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| {
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "could not create lock file {}: {}",
                lock_path.display(),
                e
            ))
        })?;
    write!(file, "{}", pid)?;
    file.sync_all()?;

    Ok(LockGuard {
        path: lock_path,
        own_pid: true,
    })
}

/// Guard returned by `acquire`. Releases the lock on drop.
pub struct LockGuard {
    path: PathBuf,
    own_pid: bool,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if self.own_pid {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn read_pid(lock_path: &Path) -> std::io::Result<u32> {
    let mut buf = String::new();
    std::fs::File::open(lock_path)?.read_to_string(&mut buf)?;
    buf.trim()
        .parse::<u32>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Cross-platform "is PID alive?" check using only stdlib.
#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    use std::process::Command;
    // `kill -0 <pid>` exits 0 if the process exists. Portable across
    // macOS + Linux without pulling in libc.
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use std::process::Command;
    // `tasklist /FI "PID eq <pid>"` — parse output for presence.
    // Good enough for MVP; swap to a proper OpenProcess call later.
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid)])
        .output()
        .map(|out| {
            let s = String::from_utf8_lossy(&out.stdout);
            s.contains(&pid.to_string())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release() {
        let dir = std::env::temp_dir().join(format!("vllora-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        {
            let _guard = acquire(&dir).expect("acquire should succeed on a fresh dir");
            assert!(dir.join(LOCK_FILE).exists());
        }
        assert!(!dir.join(LOCK_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_lock_is_reclaimed() {
        let dir = std::env::temp_dir().join(format!("vllora-stale-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake_pid: u32 = 2_000_000_000;
        std::fs::write(dir.join(LOCK_FILE), fake_pid.to_string()).unwrap();
        let _guard = acquire(&dir).expect("should reclaim stale lock");
        let pid = read_pid(&dir.join(LOCK_FILE)).unwrap();
        assert_eq!(pid, std::process::id());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nonexistent_dir_errors() {
        let dir = std::env::temp_dir().join("this-path-should-not-exist-vllora");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(acquire(&dir).is_err());
    }
}
