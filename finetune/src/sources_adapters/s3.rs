//! s3:// adapter — AWS S3.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! **MVP stub**: satisfies the `SourceAdapter` trait so verbs can route on
//! scheme without panicking. Returns a clear `not implemented` error when
//! called. Auth env vars it WILL read (documented for coworker handoff):
//! `AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY`.

use std::path::PathBuf;
use std::pin::Pin;

use super::{Result, SourceAdapter};

pub struct S3Adapter;

impl S3Adapter {
    pub fn new() -> Self {
        Self
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
            Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "s3:// adapter is not implemented yet (uri: {}). Coworker handoff: implement via `AWS S3` client reading `AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY`.",
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
        let adapter = S3Adapter::new();
        let err = adapter.resolve("s3://example/fixture").await;
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("not implemented"));
    }
}
