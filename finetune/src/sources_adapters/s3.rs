//! s3:// adapter — AWS S3 (and S3-compatible) object store.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! URI shape: `s3://<bucket>/<key>`.
//!
//! The adapter translates the URI to an HTTPS URL and delegates the download
//! to `http_fetch`. It supports:
//!
//! 1. **Public buckets** — anonymous GET over virtual-hosted URLs.
//! 2. **S3-compatible endpoints** — set `AWS_S3_ENDPOINT` (e.g. LocalStack,
//!    MinIO) + optionally `AWS_S3_FORCE_PATH_STYLE=true`.
//! 3. **Pre-signed URLs** — if the caller has one, pass it through the
//!    `https://` adapter instead; no s3:// handling needed.
//!
//! SigV4 for private buckets without a pre-signed URL is NOT wired here
//! because it requires an HMAC-SHA256 dep we haven't pulled in. When
//! `AWS_ACCESS_KEY_ID` is set we return a clear error pointing to the
//! pre-signed-URL workaround rather than silently doing anonymous GET (which
//! would return 403 on private buckets).

use std::path::PathBuf;
use std::pin::Pin;

use reqwest::header::HeaderMap;

use super::{http_fetch, Result, SourceAdapter};

pub struct S3Adapter {
    client: reqwest::Client,
}

impl S3Adapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Parse `s3://<bucket>/<key>` → (bucket, key).
    pub(crate) fn parse_uri(uri: &str) -> Result<(String, String)> {
        let rest = uri.strip_prefix("s3://").ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("not an s3:// uri: {}", uri).into()
            },
        )?;
        let (bucket, key) = rest.split_once('/').ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("s3 uri missing object key: {}", uri).into()
            },
        )?;
        if bucket.is_empty() {
            return Err(format!("s3 uri missing bucket: {}", uri).into());
        }
        if key.is_empty() {
            return Err(format!("s3 uri missing object key: {}", uri).into());
        }
        Ok((bucket.to_string(), key.to_string()))
    }

    /// Build the HTTPS URL to GET from. Honours `AWS_S3_ENDPOINT`,
    /// `AWS_S3_FORCE_PATH_STYLE`, and `AWS_REGION` env vars.
    pub(crate) fn download_url(bucket: &str, key: &str) -> String {
        let endpoint = std::env::var("AWS_S3_ENDPOINT").ok();
        let force_path_style = std::env::var("AWS_S3_FORCE_PATH_STYLE")
            .ok()
            .is_some_and(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"));

        if let Some(ep) = endpoint {
            let ep = ep.trim_end_matches('/');
            if force_path_style {
                return format!("{}/{}/{}", ep, bucket, key);
            }
            // Virtual-hosted style against custom endpoint. Expect endpoint
            // to include a scheme; otherwise fall back to path-style.
            match ep.strip_prefix("https://").or_else(|| ep.strip_prefix("http://")) {
                Some(host) => {
                    let scheme = if ep.starts_with("https://") { "https" } else { "http" };
                    format!("{}://{}.{}/{}", scheme, bucket, host, key)
                }
                None => format!("{}/{}/{}", ep, bucket, key),
            }
        } else {
            let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
            // us-east-1 supports the region-less virtual-hosted form; every
            // other region requires the regional subdomain.
            if region == "us-east-1" {
                format!("https://{}.s3.amazonaws.com/{}", bucket, key)
            } else {
                format!("https://{}.s3.{}.amazonaws.com/{}", bucket, region, key)
            }
        }
    }

    fn check_unsupported_auth() -> Result<()> {
        if std::env::var("AWS_ACCESS_KEY_ID").is_ok() {
            return Err(
                "s3:// adapter received AWS_ACCESS_KEY_ID but cannot sign requests \
                 (SigV4 not wired). Use a pre-signed URL via https:// instead, \
                 or point AWS_S3_ENDPOINT at a public mirror."
                    .into(),
            );
        }
        Ok(())
    }
}

impl Default for S3Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceAdapter for S3Adapter {
    fn scheme(&self) -> &'static str {
        "s3"
    }

    fn resolve<'a>(
        &'a self,
        uri: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            Self::check_unsupported_auth()?;
            let (bucket, key) = Self::parse_uri(uri)?;
            let url = Self::download_url(&bucket, &key);
            let filename = http_fetch::filename_from_uri(&key);
            http_fetch::fetch_to_cache(&self.client, "s3", uri, &url, &filename, HeaderMap::new())
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bucket_and_key() {
        let (b, k) = S3Adapter::parse_uri("s3://my-bucket/path/to/file.jsonl").unwrap();
        assert_eq!(b, "my-bucket");
        assert_eq!(k, "path/to/file.jsonl");
    }

    #[test]
    fn rejects_missing_key_or_bucket() {
        assert!(S3Adapter::parse_uri("s3://bucket-only").is_err());
        assert!(S3Adapter::parse_uri("s3:///key-only").is_err());
        assert!(S3Adapter::parse_uri("https://example").is_err());
    }
}
