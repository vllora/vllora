//! LocalAdapter — `file://` + bare paths.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! No download, no cache — the path is already local. Strips the `file://`
//! scheme when present; passes bare paths through after canonicalisation.

use std::path::PathBuf;
use std::pin::Pin;

use super::{Result, SourceAdapter};

pub struct LocalAdapter;

impl LocalAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceAdapter for LocalAdapter {
    fn scheme(&self) -> &'static str {
        "file"
    }

    fn resolve<'a>(
        &'a self,
        uri: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            let raw = uri.strip_prefix("file://").unwrap_or(uri);
            let path = PathBuf::from(raw);
            if !path.exists() {
                return Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "local path does not exist: {}",
                    path.display()
                )));
            }
            Ok(path)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn strips_file_scheme() {
        let adapter = LocalAdapter::new();
        let tmp = std::env::temp_dir().join(format!("local-adapter-{}.txt", std::process::id()));
        std::fs::write(&tmp, "x").unwrap();
        let uri = format!("file://{}", tmp.display());
        let resolved = adapter.resolve(&uri).await.unwrap();
        assert_eq!(resolved, tmp);
        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn bare_path_works() {
        let adapter = LocalAdapter::new();
        let tmp = std::env::temp_dir().join(format!("local-adapter-bare-{}.txt", std::process::id()));
        std::fs::write(&tmp, "x").unwrap();
        let resolved = adapter.resolve(tmp.to_str().unwrap()).await.unwrap();
        assert_eq!(resolved, tmp);
        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn nonexistent_errors() {
        let adapter = LocalAdapter::new();
        let err = adapter.resolve("/this/path/does/not/exist/ever").await;
        assert!(err.is_err());
    }
}
