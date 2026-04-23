//! azblob:// adapter — Azure Blob Storage.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! **MVP stub**: satisfies the `SourceAdapter` trait so verbs can route on
//! scheme without panicking. Returns a clear `not implemented` error when
//! called. Auth env vars it WILL read (documented for coworker handoff):
//! `AZURE_STORAGE_CONNECTION_STRING`.

use std::path::PathBuf;
use std::pin::Pin;

use super::{Result, SourceAdapter};

pub struct AzblobAdapter;

impl AzblobAdapter {
    pub fn new() -> Self {
        Self
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
            Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "azblob:// adapter is not implemented yet (uri: {}). Coworker handoff: implement via `Azure Blob Storage` client reading `AZURE_STORAGE_CONNECTION_STRING`.",
                uri
            )))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_not_implemented_error() {
        let adapter = AzblobAdapter::new();
        let err = adapter.resolve("azblob://example/fixture").await;
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("not implemented"));
    }
}
