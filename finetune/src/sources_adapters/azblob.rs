//! azblob:// adapter — Azure Blob Storage.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! URI shape: `azblob://<account>/<container>/<blob-path>`.
//!
//! Translates to `https://<account>.blob.core.windows.net/<container>/<blob>`
//! and delegates to `http_fetch`. Auth: reads `AZURE_STORAGE_SAS_TOKEN` and
//! appends it as a query string when present (SAS tokens are self-contained).
//!
//! Shared-key and connection-string auth (`AZURE_STORAGE_CONNECTION_STRING`)
//! require HMAC-SHA256 signing which we haven't pulled in. When the
//! connection string is set without a companion SAS token we return a clear
//! error pointing to the SAS-token workaround.

use std::path::PathBuf;
use std::pin::Pin;

use reqwest::header::HeaderMap;

use super::{http_fetch, Result, SourceAdapter};

pub struct AzblobAdapter {
    client: reqwest::Client,
}

impl AzblobAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Parse `azblob://<account>/<container>/<blob>` → (account, container, blob).
    pub(crate) fn parse_uri(uri: &str) -> Result<(String, String, String)> {
        let rest = uri.strip_prefix("azblob://").ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("not an azblob:// uri: {}", uri).into()
            },
        )?;

        let mut parts = rest.splitn(3, '/');
        let account = parts.next().filter(|s| !s.is_empty()).ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("azblob uri missing account: {}", uri).into()
            },
        )?;
        let container = parts.next().filter(|s| !s.is_empty()).ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("azblob uri missing container: {}", uri).into()
            },
        )?;
        let blob = parts.next().filter(|s| !s.is_empty()).ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("azblob uri missing blob path: {}", uri).into()
            },
        )?;
        Ok((account.to_string(), container.to_string(), blob.to_string()))
    }

    pub(crate) fn download_url(account: &str, container: &str, blob: &str) -> String {
        format!(
            "https://{}.blob.core.windows.net/{}/{}",
            account, container, blob
        )
    }

    /// If a SAS token is set, return `url?<sas>` (SAS tokens carry their own
    /// auth so no Authorization header is needed). Otherwise return the URL
    /// unchanged and rely on public-container access.
    fn apply_sas(url: String) -> Result<String> {
        if let Ok(sas) = std::env::var("AZURE_STORAGE_SAS_TOKEN") {
            let trimmed = sas.trim_start_matches('?');
            if trimmed.is_empty() {
                return Ok(url);
            }
            let sep = if url.contains('?') { '&' } else { '?' };
            return Ok(format!("{}{}{}", url, sep, trimmed));
        }
        if std::env::var("AZURE_STORAGE_CONNECTION_STRING").is_ok()
            || std::env::var("AZURE_STORAGE_ACCOUNT_KEY").is_ok()
        {
            return Err(
                "azblob:// adapter sees connection-string/account-key creds but \
                 cannot HMAC-sign requests (shared-key signing not wired). Generate \
                 a SAS token (Azure portal or `az storage container generate-sas`) \
                 and export it as AZURE_STORAGE_SAS_TOKEN."
                    .into(),
            );
        }
        Ok(url)
    }
}

impl Default for AzblobAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceAdapter for AzblobAdapter {
    fn scheme(&self) -> &'static str {
        "azblob"
    }

    fn resolve<'a>(
        &'a self,
        uri: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            let (account, container, blob) = Self::parse_uri(uri)?;
            let base_url = Self::download_url(&account, &container, &blob);
            let url = Self::apply_sas(base_url)?;
            let filename = http_fetch::filename_from_uri(&blob);
            http_fetch::fetch_to_cache(
                &self.client,
                "azblob",
                uri,
                &url,
                &filename,
                HeaderMap::new(),
            )
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_azblob_uri() {
        let (acct, cont, blob) =
            AzblobAdapter::parse_uri("azblob://myacct/mycontainer/path/to/blob.csv").unwrap();
        assert_eq!(acct, "myacct");
        assert_eq!(cont, "mycontainer");
        assert_eq!(blob, "path/to/blob.csv");
    }

    #[test]
    fn rejects_missing_segments() {
        assert!(AzblobAdapter::parse_uri("azblob://acct").is_err());
        assert!(AzblobAdapter::parse_uri("azblob://acct/container").is_err());
        assert!(AzblobAdapter::parse_uri("azblob:///container/blob").is_err());
        assert!(AzblobAdapter::parse_uri("https://example").is_err());
    }

    #[test]
    fn download_url_is_standard_blob_endpoint() {
        assert_eq!(
            AzblobAdapter::download_url("myacct", "mycontainer", "folder/data.jsonl"),
            "https://myacct.blob.core.windows.net/mycontainer/folder/data.jsonl"
        );
    }
}
