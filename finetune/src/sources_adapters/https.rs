//! https:// adapter — HTTPS download.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! **MVP stub**: satisfies the `SourceAdapter` trait so verbs can route on
//! scheme without panicking. Returns a clear `not implemented` error when
//! called. Auth env vars it WILL read (documented for coworker handoff):
//! `(no auth)`.

use std::path::PathBuf;
use std::pin::Pin;

use super::{Result, SourceAdapter};

pub struct HttpsAdapter;

impl HttpsAdapter {
    pub fn new() -> Self {
        Self
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
            Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "https:// adapter is not implemented yet (uri: {}). Coworker handoff: implement via `HTTPS download` client reading `(no auth)`.",
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
        let adapter = HttpsAdapter::new();
        let err = adapter.resolve("https://example/fixture").await;
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("not implemented"));
    }
}
