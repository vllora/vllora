//! Plugin symlink management.
//!
//! Track: C | Feature: 005-install-flow
//!
//! Ensures `~/.claude/plugins/vllora-finetune/` symlinks to the bundled plugin
//! directory. Idempotent — noop if the symlink already points at the expected
//! target. Never overwrites an existing symlink that points elsewhere (the user
//! may have installed a fork / dev variant intentionally).

use crate::CliError;
use std::path::{Path, PathBuf};

const PLUGIN_DIR_ENV: &str = "VLLORA_PLUGIN_DIR";
const PLUGIN_SUBDIR: &str = "plugin";
const TARGET_DIR_NAME: &str = "vllora-finetune";

/// Resolve the bundled plugin directory path, in order of preference:
///   1. `VLLORA_PLUGIN_DIR` env var (CI / tests / custom installs).
///   2. Maturin wheel layout: `<exe>/../../share/vllora/plugin` (pip-installed).
///   3. Cargo workspace: `<exe>/../../../plugin` (development via `cargo run`).
///
/// Returns an error only if none of the candidates exist. The first candidate
/// that resolves to an existing directory wins.
fn plugin_source_dir() -> Result<PathBuf, CliError> {
    for candidate in plugin_source_candidates()? {
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    Err(CliError::CustomError(format!(
        "plugin directory not found. Set {} or reinstall vllora.",
        PLUGIN_DIR_ENV
    )))
}

fn plugin_source_candidates() -> Result<Vec<PathBuf>, CliError> {
    let mut candidates = Vec::new();

    if let Ok(dir) = std::env::var(PLUGIN_DIR_ENV) {
        candidates.push(PathBuf::from(dir));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // Wheel layout (maturin `data/` → `share/vllora/plugin`):
            //   <prefix>/bin/vllora
            //   <prefix>/share/vllora/plugin/
            if let Some(prefix) = exe_dir.parent() {
                candidates.push(prefix.join("share").join("vllora").join(PLUGIN_SUBDIR));
            }

            // Cargo dev layout:
            //   <repo>/target/debug/vllora or <repo>/target/release/vllora
            //   <repo>/plugin/
            if let Some(target_dir) = exe_dir.parent() {
                if let Some(repo_root) = target_dir.parent() {
                    candidates.push(repo_root.join(PLUGIN_SUBDIR));
                }
            }
        }
    }

    Ok(candidates)
}

/// Resolve the target symlink location: `~/.claude/plugins/vllora-finetune/`.
fn plugin_target_dir() -> Result<PathBuf, CliError> {
    let home = std::env::var("HOME")
        .map_err(|_| CliError::CustomError("$HOME not set".into()))?;
    Ok(PathBuf::from(home).join(".claude").join("plugins").join(TARGET_DIR_NAME))
}

/// Ensure the symlink exists and points at the bundled plugin.
///
/// Outcomes:
/// - Target doesn't exist                                → create symlink.
/// - Target is a symlink to the expected source          → noop.
/// - Target is a symlink to something else               → warn on stderr, don't overwrite.
/// - Target is a regular file/directory (not a symlink)  → warn on stderr, don't overwrite.
pub fn ensure() -> Result<(), CliError> {
    let src = plugin_source_dir()?;
    let tgt = plugin_target_dir()?;

    // Ensure the parent `.claude/plugins/` directory exists.
    if let Some(parent) = tgt.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match std::fs::symlink_metadata(&tgt) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            create_symlink(&src, &tgt)?;
        }
        Err(e) => return Err(CliError::IoError(e)),
        Ok(meta) if meta.file_type().is_symlink() => {
            let current = std::fs::read_link(&tgt)?;
            if paths_equivalent(&current, &src) {
                // Idempotent: already pointing where we want.
            } else {
                eprintln!(
                    "Warning: plugin symlink at {} points to {} (expected {}). Leaving as-is. \
                     Run `rm {}` and re-run vllora to reset.",
                    tgt.display(),
                    current.display(),
                    src.display(),
                    tgt.display(),
                );
            }
        }
        Ok(_) => {
            eprintln!(
                "Warning: {} exists but is not a symlink. Leaving as-is.",
                tgt.display()
            );
        }
    }

    Ok(())
}

/// Remove the symlink (for `vllora doctor --clean` or uninstall flows — future).
#[allow(dead_code)]
pub fn remove() -> Result<(), CliError> {
    let tgt = plugin_target_dir()?;
    match std::fs::symlink_metadata(&tgt) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(CliError::IoError(e)),
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::remove_file(&tgt)?;
            Ok(())
        }
        Ok(_) => Err(CliError::CustomError(format!(
            "{} is not a symlink; refusing to remove",
            tgt.display()
        ))),
    }
}

#[cfg(unix)]
fn create_symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(windows)]
fn create_symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(src, dst)
}

fn paths_equivalent(a: &Path, b: &Path) -> bool {
    // Both paths may be relative symlinks or absolute; compare after canonicalising
    // where possible. Fall back to raw equality if canonicalise fails (e.g., if the
    // symlink target was removed while we weren't looking).
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Env-var tests share process state — consolidated into one sequential
    // function so cargo's default parallel runner can't race them.
    #[test]
    fn env_and_symlink_behaviour() {
        // Case 1: env override shows up in candidate list.
        std::env::set_var(PLUGIN_DIR_ENV, "/tmp/fake-plugin");
        let candidates = plugin_source_candidates().unwrap();
        assert!(candidates.iter().any(|p| p == Path::new("/tmp/fake-plugin")));

        // Case 2: target dir is `$HOME/.claude/plugins/vllora-finetune`.
        let tmp = std::env::temp_dir().join(format!("vllora-plugin-test-{}", std::process::id()));
        let fake_home = tmp.join("home");
        let fake_plugin = tmp.join("plugin");
        std::fs::create_dir_all(&fake_plugin).unwrap();
        std::fs::create_dir_all(&fake_home).unwrap();

        std::env::set_var("HOME", &fake_home);
        std::env::set_var(PLUGIN_DIR_ENV, &fake_plugin);

        let tgt = plugin_target_dir().unwrap();
        assert_eq!(tgt, fake_home.join(".claude/plugins/vllora-finetune"));

        // Case 3: ensure() is idempotent — first call creates, second is noop.
        ensure().unwrap();
        assert!(tgt.exists());
        assert!(std::fs::symlink_metadata(&tgt).unwrap().file_type().is_symlink());
        ensure().unwrap();
        // Second call must not have broken or re-created the link.
        assert!(std::fs::symlink_metadata(&tgt).unwrap().file_type().is_symlink());

        std::fs::remove_dir_all(&tmp).ok();
        std::env::remove_var(PLUGIN_DIR_ENV);
    }
}
