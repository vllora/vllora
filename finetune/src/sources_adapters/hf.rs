//! hf:// adapter — HuggingFace Hub.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! **MVP stub**: satisfies the `SourceAdapter` trait so verbs can route on
//! scheme without panicking. Returns a clear `not implemented` error when
//! called. Auth env vars it WILL read (documented for coworker handoff):
//! `HF_TOKEN`.

use std::path::PathBuf;
use std::pin::Pin;

use super::{Result, SourceAdapter};

pub struct HfAdapter;

impl HfAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HfAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceAdapter for HfAdapter {
    fn scheme(&self) -> &'static str {
        "hf"
    }

    fn resolve<'a>(
        &'a self,
        uri: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "hf:// adapter is not implemented yet (uri: {}). Coworker handoff: implement via `HuggingFace Hub` client reading `HF_TOKEN`.",
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
        let adapter = HfAdapter::new();
        let err = adapter.resolve("hf://example/fixture").await;
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("not implemented"));
    }
}
