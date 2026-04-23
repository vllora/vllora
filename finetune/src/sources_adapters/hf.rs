//! hf:// adapter — HuggingFace Hub.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §4.5
//!
//! URI shape: `hf://<org>/<repo>[@<ref>]/<path>` (ref defaults to `main`).
//! Repo type defaults to `datasets`; use `hf://models/<org>/<repo>@<ref>/<path>`
//! to target a model repo instead.
//!
//! Translates the URI to the Hub download URL
//! `https://huggingface.co/<maybe-models>/<org>/<repo>/resolve/<ref>/<path>`
//! and delegates the actual download + cache to `http_fetch`. Reads
//! `HF_TOKEN` for gated/private repos; public files work without it.

use std::path::PathBuf;
use std::pin::Pin;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};

use super::{http_fetch, Result, SourceAdapter};

pub struct HfAdapter {
    client: reqwest::Client,
}

impl HfAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Parse `hf://<org>/<repo>[@<ref>]/<path>` into (repo_type, org, repo, ref, path).
    /// `repo_type` is `datasets` unless the path starts with `models/`.
    pub(crate) fn parse_uri(uri: &str) -> Result<ParsedHfUri> {
        let rest = uri.strip_prefix("hf://").ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
            format!("not an hf:// uri: {}", uri).into()
        })?;

        let (repo_type, tail) = if let Some(stripped) = rest.strip_prefix("models/") {
            ("models", stripped)
        } else {
            ("datasets", rest)
        };

        // Split into at most 3 parts: org / repo / path...
        let mut parts = tail.splitn(3, '/');
        let org = parts.next().filter(|s| !s.is_empty()).ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("hf uri missing org segment: {}", uri).into()
            },
        )?;
        let repo_ref = parts.next().filter(|s| !s.is_empty()).ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("hf uri missing repo segment: {}", uri).into()
            },
        )?;
        let path = parts.next().filter(|s| !s.is_empty()).ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                format!("hf uri missing file path: {}", uri).into()
            },
        )?;

        let (repo, git_ref) = match repo_ref.split_once('@') {
            Some((r, refname)) if !refname.is_empty() => (r, refname),
            _ => (repo_ref, "main"),
        };

        Ok(ParsedHfUri {
            repo_type: repo_type.to_string(),
            org: org.to_string(),
            repo: repo.to_string(),
            git_ref: git_ref.to_string(),
            path: path.to_string(),
        })
    }

    pub(crate) fn download_url(parsed: &ParsedHfUri) -> String {
        // Datasets: https://huggingface.co/datasets/<org>/<repo>/resolve/<ref>/<path>
        // Models:   https://huggingface.co/<org>/<repo>/resolve/<ref>/<path>
        if parsed.repo_type == "datasets" {
            format!(
                "https://huggingface.co/datasets/{}/{}/resolve/{}/{}",
                parsed.org, parsed.repo, parsed.git_ref, parsed.path
            )
        } else {
            format!(
                "https://huggingface.co/{}/{}/resolve/{}/{}",
                parsed.org, parsed.repo, parsed.git_ref, parsed.path
            )
        }
    }

    fn auth_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(token) = std::env::var("HF_TOKEN") {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", token)) {
                headers.insert(AUTHORIZATION, val);
            }
        }
        headers
    }
}

pub(crate) struct ParsedHfUri {
    pub repo_type: String,
    pub org: String,
    pub repo: String,
    pub git_ref: String,
    pub path: String,
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
            let parsed = Self::parse_uri(uri)?;
            let url = Self::download_url(&parsed);
            let filename = http_fetch::filename_from_uri(&parsed.path);
            let headers = Self::auth_headers();
            http_fetch::fetch_to_cache(&self.client, "hf", uri, &url, &filename, headers).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dataset_uri_with_default_ref() {
        let p = HfAdapter::parse_uri("hf://anthropic/hh-rlhf/train.jsonl").unwrap();
        assert_eq!(p.repo_type, "datasets");
        assert_eq!(p.org, "anthropic");
        assert_eq!(p.repo, "hh-rlhf");
        assert_eq!(p.git_ref, "main");
        assert_eq!(p.path, "train.jsonl");
    }

    #[test]
    fn parses_dataset_uri_with_explicit_ref_and_subdir() {
        let p = HfAdapter::parse_uri("hf://anthropic/hh-rlhf@v1.0/data/train.jsonl").unwrap();
        assert_eq!(p.git_ref, "v1.0");
        assert_eq!(p.path, "data/train.jsonl");
    }

    #[test]
    fn parses_model_repo_prefix() {
        let p = HfAdapter::parse_uri("hf://models/meta-llama/Llama-3-8B/config.json").unwrap();
        assert_eq!(p.repo_type, "models");
        assert_eq!(p.org, "meta-llama");
        assert_eq!(p.repo, "Llama-3-8B");
        assert_eq!(p.path, "config.json");
    }

    #[test]
    fn rejects_malformed_uri() {
        assert!(HfAdapter::parse_uri("hf://anthropic").is_err());
        assert!(HfAdapter::parse_uri("hf://anthropic/hh-rlhf").is_err());
        assert!(HfAdapter::parse_uri("https://example.com").is_err());
    }

    #[test]
    fn builds_canonical_hub_url() {
        let p = HfAdapter::parse_uri("hf://anthropic/hh-rlhf@main/data.jsonl").unwrap();
        assert_eq!(
            HfAdapter::download_url(&p),
            "https://huggingface.co/datasets/anthropic/hh-rlhf/resolve/main/data.jsonl"
        );

        let p = HfAdapter::parse_uri("hf://models/meta-llama/Llama-3-8B/config.json").unwrap();
        assert_eq!(
            HfAdapter::download_url(&p),
            "https://huggingface.co/meta-llama/Llama-3-8B/resolve/main/config.json"
        );
    }
}
