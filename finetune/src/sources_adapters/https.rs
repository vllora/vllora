//! `https://` adapter — HTTPS download with content-hash caching.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! Uses the already-present `reqwest` client. Downloads to
//! `~/.vllora/cache/sources/https/<url-hash>/<filename>`; cache-hit avoids
//! re-downloading. No auth — public resources only. For authenticated
//! downloads, prefer the dedicated scheme adapters (`hf://`, `s3://`, etc.).

use std::path::PathBuf;
use std::pin::Pin;

use super::{Result, SourceAdapter};

pub struct HttpsAdapter {
    client: reqwest::Client,
    cache_dir: PathBuf,
}

impl HttpsAdapter {
    pub fn new() -> Self {
        let cache_dir = Self::default_cache_dir();
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            cache_dir,
        }
    }

    fn default_cache_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join(".vllora")
            .join("cache")
            .join("sources")
            .join("https")
    }

    #[doc(hidden)]
    pub fn with_cache_dir(cache_dir: PathBuf) -> Self {
        Self {
            client: reqwest::Client::new(),
            cache_dir,
        }
    }

    /// Compute a stable cache subdir name for a URL. Uses a simple hash
    /// based on the URL bytes so we don't pull in a crypto dep. Not
    /// collision-resistant, but fine for a local cache.
    fn url_hash(url: &str) -> String {
        // DefaultHasher is not stable across Rust versions, which is fine
        // for a cache (stale entries just re-download).
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        url.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    fn filename_from_url(url: &str) -> String {
        url.rsplit_once('/')
            .map(|(_, tail)| tail)
            .filter(|s| !s.is_empty())
            .and_then(|s| s.split_once('?').map(|(n, _)| n).or(Some(s)))
            .unwrap_or("download.bin")
            .to_string()
    }
}

impl Default for HttpsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceAdapter for HttpsAdapter {
    fn scheme(&self) -> &'static str {
        "https"
    }

    fn resolve<'a>(
        &'a self,
        uri: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            if !uri.starts_with("http://") && !uri.starts_with("https://") {
                return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "HttpsAdapter given non-http(s) URI: {}",
                    uri
                )));
            }

            let hash = Self::url_hash(uri);
            let filename = Self::filename_from_url(uri);
            let target_dir = self.cache_dir.join(&hash);
            let target = target_dir.join(&filename);

            // Cache hit.
            if target.is_file() {
                return Ok(target);
            }

            std::fs::create_dir_all(&target_dir)?;

            let resp = self.client.get(uri).send().await?;
            if !resp.status().is_success() {
                return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "{} returned HTTP {}",
                    uri,
                    resp.status()
                )));
            }
            let bytes = resp.bytes().await?;

            // Write atomically to avoid readers seeing a partial file after an
            // interrupted download.
            let tmp = target_dir.join(format!(".{}.tmp.{}", filename, std::process::id()));
            std::fs::write(&tmp, &bytes)?;
            std::fs::rename(&tmp, &target)?;

            Ok(target)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_hash_is_stable_per_run() {
        let a = HttpsAdapter::url_hash("https://example.com/fixture.zip");
        let b = HttpsAdapter::url_hash("https://example.com/fixture.zip");
        assert_eq!(a, b);
    }

    #[test]
    fn filename_from_url_strips_query() {
        assert_eq!(
            HttpsAdapter::filename_from_url("https://example.com/a/b/fixture.zip?v=1"),
            "fixture.zip"
        );
        assert_eq!(
            HttpsAdapter::filename_from_url("https://example.com/"),
            "download.bin"
        );
        assert_eq!(
            HttpsAdapter::filename_from_url("https://example.com/noslash"),
            "noslash"
        );
    }

    #[tokio::test]
    async fn non_http_uri_rejected() {
        let adapter = HttpsAdapter::new();
        let err = adapter.resolve("file:///tmp/x").await;
        assert!(err.is_err());
        assert!(format!("{}", err.unwrap_err()).contains("non-http"));
    }

    #[tokio::test]
    async fn cache_hit_on_second_resolve() {
        // Pre-populate the cache to simulate a previous download.
        let tmp = std::env::temp_dir().join(format!(
            "https-adapter-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let adapter = HttpsAdapter::with_cache_dir(tmp.clone());
        let url = "https://example.com/fake/data.bin";
        let hash = HttpsAdapter::url_hash(url);
        let target_dir = tmp.join(&hash);
        std::fs::create_dir_all(&target_dir).unwrap();
        let target = target_dir.join("data.bin");
        std::fs::write(&target, b"cached").unwrap();

        // No network call because the cache entry exists.
        let resolved = adapter.resolve(url).await.unwrap();
        assert_eq!(resolved, target);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
