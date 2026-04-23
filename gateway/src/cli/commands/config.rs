//! `vllora config get/set` — `~/.vllora/config.yaml` management.
//!
//! Track: C | Feature: 005-install-flow | Design: parent §2.8
//!
//! Minimal dot-path reader/writer over a flat YAML mapping. Good enough for v0
//! — expand when we know which config keys users actually reach for.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use vllora_core::metadata::pool::DbPool;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    #[command(subcommand)]
    pub op: ConfigOp,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigOp {
    /// Print a config value (empty string if unset).
    Get { key: String },
    /// Set a config value.
    Set { key: String, value: String },
}

pub async fn handle_config(_db_pool: DbPool, args: Args) -> Result<(), crate::CliError> {
    match args.op {
        ConfigOp::Get { key } => {
            let cfg = load_config()?;
            let value = lookup(&cfg, &key).unwrap_or_else(|| "".into());
            println!("{}", value);
            Ok(())
        }
        ConfigOp::Set { key, value } => {
            let mut cfg = load_config()?;
            set(&mut cfg, &key, serde_yaml::Value::String(value));
            save_config(&cfg)?;
            Ok(())
        }
    }
}

fn config_path() -> Result<PathBuf, crate::CliError> {
    let home = std::env::var("HOME")
        .map_err(|_| crate::CliError::CustomError("$HOME not set".into()))?;
    Ok(PathBuf::from(home).join(".vllora").join("config.yaml"))
}

fn load_config() -> Result<serde_yaml::Value, crate::CliError> {
    let path = config_path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) if !content.trim().is_empty() => Ok(serde_yaml::from_str(&content)?),
        _ => Ok(serde_yaml::Value::Mapping(Default::default())),
    }
}

fn save_config(cfg: &serde_yaml::Value) -> Result<(), crate::CliError> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let yaml = serde_yaml::to_string(cfg)?;
    std::fs::write(&path, yaml)?;
    Ok(())
}

/// Look up a dot-separated key path. Returns the value as a string, or `None`.
fn lookup(cfg: &serde_yaml::Value, key: &str) -> Option<String> {
    let mut current = cfg;
    for segment in key.split('.') {
        current = current.as_mapping()?.get(serde_yaml::Value::String(segment.into()))?;
    }
    match current {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => serde_yaml::to_string(current).ok().map(|s| s.trim().to_string()),
    }
}

/// Set a dot-separated key path. Creates intermediate maps as needed. Overwrites
/// any existing value at the leaf — v0 doesn't try to merge.
fn set(cfg: &mut serde_yaml::Value, key: &str, value: serde_yaml::Value) {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return;
    }

    // Ensure root is a mapping.
    if !cfg.is_mapping() {
        *cfg = serde_yaml::Value::Mapping(Default::default());
    }

    let mut current = cfg;
    for segment in &parts[..parts.len() - 1] {
        let map = current.as_mapping_mut().expect("ensured above");
        let key_v = serde_yaml::Value::String(segment.to_string());
        if !map.contains_key(&key_v) {
            map.insert(key_v.clone(), serde_yaml::Value::Mapping(Default::default()));
        }
        current = map.get_mut(&key_v).expect("just inserted");
        if !current.is_mapping() {
            *current = serde_yaml::Value::Mapping(Default::default());
        }
    }

    let leaf = parts.last().unwrap();
    if let Some(map) = current.as_mapping_mut() {
        map.insert(serde_yaml::Value::String(leaf.to_string()), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_missing_returns_none() {
        let cfg = serde_yaml::Value::Mapping(Default::default());
        assert!(lookup(&cfg, "cache.max_gb").is_none());
    }

    #[test]
    fn set_and_get_roundtrip() {
        let mut cfg = serde_yaml::Value::Mapping(Default::default());
        set(&mut cfg, "cache.max_gb", serde_yaml::Value::String("20".into()));
        assert_eq!(lookup(&cfg, "cache.max_gb"), Some("20".into()));
    }

    #[test]
    fn set_overwrites_non_map_intermediate() {
        let mut cfg = serde_yaml::Value::Mapping(Default::default());
        set(&mut cfg, "a", serde_yaml::Value::String("x".into()));
        // Now replace "a" with a nested structure; set() should promote.
        set(&mut cfg, "a.b", serde_yaml::Value::String("y".into()));
        assert_eq!(lookup(&cfg, "a.b"), Some("y".into()));
    }
}
