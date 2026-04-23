//! Shared HTTP-fetch-with-cache helper used by every remote URI adapter.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! Adapters only need to translate their scheme-specific URI to an HTTPS URL
//! plus optional auth headers; this module handles the network + cache layout
//! (`~/.vllora/cache/sources/<scheme>/<hash>/<filename>`) and atomic writes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use reqwest::header::HeaderMap;

use super::Result;

/// Compute a stable cache subdir name for a URI. Uses a non-crypto hash
/// because collisions at worst trigger a re-download.
pub fn uri_hash(uri: &str) -> String {
    let mut h = DefaultHasher::new();
    uri.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Pull the last path segment (before any `?query`) as the cached filename.
/// Falls back to `download.bin` for URIs that don't expose a filename.
pub fn filename_from_uri(uri: &str) -> String {
    uri.rsplit_once('/')
        .map(|(_, tail)| tail)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.split_once('?').map(|(n, _)| n).or(Some(s)))
        .unwrap_or("download.bin")
        .to_string()
}

fn default_cache_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".vllora")
        .join("cache")
        .join("sources")
}

/// Fetch `url` with `headers` and cache the result under
/// `<cache_root>/<scheme>/<hash>/<filename>`.
///
/// * `scheme` — cache bucket name (e.g. "hf", "s3"). Keeps per-scheme caches
///   isolated so mismatched auth doesn't poison a shared cache.
/// * `cache_key` — string hashed to form the cache subdir. Usually the
///   original scheme-specific URI (so two different aliases for the same
///   object don't collide).
/// * `filename` — filename used inside the subdir; clients typically derive
///   this from the last path segment.
pub async fn fetch_to_cache(
    client: &reqwest::Client,
    scheme: &str,
    cache_key: &str,
    url: &str,
    filename: &str,
    headers: HeaderMap,
) -> Result<PathBuf> {
    fetch_to_cache_in(&default_cache_root(), client, scheme, cache_key, url, filename, headers).await
}

/// Test-hook variant that lets callers override the cache root.
pub async fn fetch_to_cache_in(
    cache_root: &std::path::Path,
    client: &reqwest::Client,
    scheme: &str,
    cache_key: &str,
    url: &str,
    filename: &str,
    headers: HeaderMap,
) -> Result<PathBuf> {
    let hash = uri_hash(cache_key);
    let target_dir = cache_root.join(scheme).join(&hash);
    let target = target_dir.join(filename);

    if target.is_file() {
        return Ok(target);
    }

    std::fs::create_dir_all(&target_dir)?;

    let resp = client.get(url).headers(headers).send().await?;
    if !resp.status().is_success() {
        return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
            "{} returned HTTP {}",
            url,
            resp.status()
        )));
    }
    let bytes = resp.bytes().await?;

    let tmp = target_dir.join(format!(".{}.tmp.{}", filename, std::process::id()));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable() {
        assert_eq!(uri_hash("x"), uri_hash("x"));
    }

    #[test]
    fn filename_extraction_handles_query_and_trailing_slash() {
        assert_eq!(filename_from_uri("hf://org/name/resolve/main/data.jsonl"), "data.jsonl");
        assert_eq!(filename_from_uri("s3://bucket/a/b/c.csv?versionId=abc"), "c.csv");
        assert_eq!(filename_from_uri("gs://bucket/"), "download.bin");
    }
}
