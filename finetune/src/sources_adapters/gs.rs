//! gs:// adapter — Google Cloud Storage.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! URI shape: `gs://<bucket>/<object>`.
//!
//! Translates to the JSON-API download URL
//! `https://storage.googleapis.com/<bucket>/<object>` and delegates to
//! `http_fetch`. Auth: reads `GCS_OAUTH_TOKEN` (or `GOOGLE_OAUTH_TOKEN`) for
//! private objects. Minting a bearer from `GOOGLE_APPLICATION_CREDENTIALS`
//! requires the `google-cloud-auth` dep which we haven't pulled in — if that
//! env var is set without a companion token var, the adapter returns a clear
//! error with the workaround (pre-mint the token and pass it in).

use std::path::PathBuf;
use std::pin::Pin;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};

use super::{http_fetch, Result, SourceAdapter};

pub struct GsAdapter {
    client: reqwest::Client,
}

impl GsAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub(crate) fn parse_uri(uri: &str) -> Result<(String, String)> {
        let rest = uri.strip_prefix("gs://").ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("not a gs:// uri: {}", uri).into()
            },
        )?;
        let (bucket, object) = rest.split_once('/').ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("gs uri missing object name: {}", uri).into()
            },
        )?;
        if bucket.is_empty() {
            return Err(format!("gs uri missing bucket: {}", uri).into());
        }
        if object.is_empty() {
            return Err(format!("gs uri missing object name: {}", uri).into());
        }
        Ok((bucket.to_string(), object.to_string()))
    }

    pub(crate) fn download_url(bucket: &str, object: &str) -> String {
        format!("https://storage.googleapis.com/{}/{}", bucket, object)
    }

    fn auth_headers() -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        if let Some(token) = std::env::var("GCS_OAUTH_TOKEN")
            .ok()
            .or_else(|| std::env::var("GOOGLE_OAUTH_TOKEN").ok())
        {
            let val = HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                    format!("invalid GCS OAuth token: {}", e).into()
                })?;
            headers.insert(AUTHORIZATION, val);
            return Ok(headers);
        }

        if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
            return Err(
                "gs:// adapter sees GOOGLE_APPLICATION_CREDENTIALS but cannot mint \
                 a bearer token itself (no cloud-auth dep). Pre-mint a token with \
                 `gcloud auth application-default print-access-token` and pass it \
                 via GCS_OAUTH_TOKEN."
                    .into(),
            );
        }
        // Anonymous GET — works for public objects.
        Ok(headers)
    }
}

impl Default for GsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceAdapter for GsAdapter {
    fn scheme(&self) -> &'static str {
        "gs"
    }

    fn resolve<'a>(
        &'a self,
        uri: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            let (bucket, object) = Self::parse_uri(uri)?;
            let url = Self::download_url(&bucket, &object);
            let filename = http_fetch::filename_from_uri(&object);
            let headers = Self::auth_headers()?;
            http_fetch::fetch_to_cache(&self.client, "gs", uri, &url, &filename, headers).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bucket_and_object() {
        let (b, o) = GsAdapter::parse_uri("gs://my-bucket/folder/file.jsonl").unwrap();
        assert_eq!(b, "my-bucket");
        assert_eq!(o, "folder/file.jsonl");
    }

    #[test]
    fn rejects_missing_parts() {
        assert!(GsAdapter::parse_uri("gs://bucket-only").is_err());
        assert!(GsAdapter::parse_uri("gs:///obj-only").is_err());
        assert!(GsAdapter::parse_uri("https://example").is_err());
    }

    #[test]
    fn download_url_is_json_api_form() {
        assert_eq!(
            GsAdapter::download_url("my-bucket", "data/train.jsonl"),
            "https://storage.googleapis.com/my-bucket/data/train.jsonl"
        );
    }
}
