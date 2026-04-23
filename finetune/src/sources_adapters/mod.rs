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
