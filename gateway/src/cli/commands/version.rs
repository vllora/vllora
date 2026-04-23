//! `vllora version` — print CLI + plugin manifest versions.
//!
//! Track: C | Feature: 005-install-flow | Design: parent §2.8

use std::path::PathBuf;

pub async fn handle_version() -> Result<(), crate::CliError> {
    let cli_version = env!("CARGO_PKG_VERSION");
    let build_target = option_env!("TARGET").unwrap_or("unknown");
    let build_profile = if cfg!(debug_assertions) { "debug" } else { "release" };

    println!("vllora {}", cli_version);
    println!("  target:   {}", build_target);
    println!("  profile:  {}", build_profile);

    match plugin_manifest_version() {
        Some(v) => println!("  plugin:   {}", v),
        None => println!("  plugin:   not installed"),
    }

    Ok(())
}

/// Read the plugin manifest at `~/.claude/plugins/vllora-finetune/plugin.json`
/// and pluck out the `version` field. Returns `None` if the symlink is missing,
/// the file is unreadable, or the JSON doesn't contain a `version` string.
fn plugin_manifest_version() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let manifest = PathBuf::from(home).join(".claude/plugins/vllora-finetune/plugin.json");
    let raw = std::fs::read_to_string(&manifest).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    parsed.get("version")?.as_str().map(String::from)
}
