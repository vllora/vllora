//! URI adapters — resolve remote sources to local cache.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! Workers receive LOCAL PATHS. Adapters authenticate via provider env
//! vars (HF_TOKEN, AWS_*, etc.) and download to ~/.vllora/cache/sources/.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub mod local;
pub mod hf;
pub mod s3;
pub mod gs;
pub mod azblob;
pub mod https;

/// Placeholder error alias used by the skeleton. Track B replaces with a
/// dedicated `AdapterError` enum when wiring real adapters.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

/// URI → local path resolver. One impl per scheme. Kept object-safe via
/// a boxed future instead of `async fn in trait` so the trait stays usable
/// behind `dyn SourceAdapter` until we decide whether to pin a specific
/// async-runtime dep.
pub trait SourceAdapter: Send + Sync {
    fn scheme(&self) -> &'static str;
    fn resolve<'a>(
        &'a self,
        uri: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<PathBuf>> + Send + 'a>>;
}

/// Parse the scheme portion of a URI. Returns `"file"` for bare paths so the
/// `LocalAdapter` is picked by default.
pub fn parse_scheme(uri: &str) -> &'static str {
    if let Some(pos) = uri.find("://") {
        match &uri[..pos] {
            "file" => "file",
            "hf" => "hf",
            "s3" => "s3",
            "gs" => "gs",
            "azblob" => "azblob",
            "https" => "https",
            "http" => "https", // treat http:// as https-class for simplicity
            _ => "file",
        }
    } else {
        "file"
    }
}

/// Resolve a URI to a local path by dispatching to the right adapter. MVP:
/// only `file://` / bare paths succeed; remote schemes return a clear
/// "not implemented" error from their adapter.
pub async fn resolve_uri(uri: &str) -> Result<PathBuf> {
    let scheme = parse_scheme(uri);
    match scheme {
        "file" => local::LocalAdapter::new().resolve(uri).await,
        "hf" => hf::HfAdapter::new().resolve(uri).await,
        "s3" => s3::S3Adapter::new().resolve(uri).await,
        "gs" => gs::GsAdapter::new().resolve(uri).await,
        "azblob" => azblob::AzblobAdapter::new().resolve(uri).await,
        "https" => https::HttpsAdapter::new().resolve(uri).await,
        other => Err(Box::<dyn std::error::Error + Send + Sync>::from(format!(
            "unsupported URI scheme: {}",
            other
        ))),
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    #[test]
    fn parses_scheme() {
        assert_eq!(parse_scheme("file:///tmp/x"), "file");
        assert_eq!(parse_scheme("/tmp/x"), "file");
        assert_eq!(parse_scheme("hf://org/name"), "hf");
        assert_eq!(parse_scheme("s3://bucket/key"), "s3");
        assert_eq!(parse_scheme("https://example.com"), "https");
        assert_eq!(parse_scheme("http://example.com"), "https");
        assert_eq!(parse_scheme("unknown://blah"), "file");
    }

    #[tokio::test]
    async fn local_uri_resolves_to_existing_file() {
        let tmp = std::env::temp_dir().join(format!("resolve-{}.txt", std::process::id()));
        std::fs::write(&tmp, "x").unwrap();
        let resolved = resolve_uri(tmp.to_str().unwrap()).await.unwrap();
        assert_eq!(resolved, tmp);
        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn hf_uri_returns_not_implemented_error() {
        let err = resolve_uri("hf://anthropic/hh-rlhf").await;
        assert!(err.is_err());
        assert!(format!("{}", err.unwrap_err()).contains("not implemented"));
    }
}
