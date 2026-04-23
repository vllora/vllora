//! gs:// adapter — Google Cloud Storage.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! **MVP stub**: satisfies the `SourceAdapter` trait so verbs can route on
//! scheme without panicking. Returns a clear `not implemented` error when
//! called. Auth env vars it WILL read (documented for coworker handoff):
//! `GOOGLE_APPLICATION_CREDENTIALS`.

use std::path::PathBuf;
use std::pin::Pin;

use super::{Result, SourceAdapter};

pub struct GsAdapter;

impl GsAdapter {
    pub fn new() -> Self {
        Self
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
            Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "gs:// adapter is not implemented yet (uri: {}). Coworker handoff: implement via `Google Cloud Storage` client reading `GOOGLE_APPLICATION_CREDENTIALS`.",
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
        let adapter = GsAdapter::new();
        let err = adapter.resolve("gs://example/fixture").await;
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("not implemented"));
    }
}
