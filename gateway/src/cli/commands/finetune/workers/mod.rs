//! `claude -p` subprocess workers for LLM-heavy pipeline steps.
//!
//! Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §6
//!
//! Each worker:
//!   - Loads its system prompt from vllora_finetune::prompts (include_str!).
//!   - Spawns `claude -p --output-format stream-json --allowedTools ... --max-turns N`.
//!   - Inherits user's Claude auth via environment (no credential file reads).
//!   - Parses stream-JSON events into typed progress + final result.
//!
//! Dependencies: tokio::process::Command, serde_json, tokio_stream.

pub mod claude_client;
pub mod knowledge_extractor;
pub mod relation_builder;
pub mod trace_analyzer;
pub mod record_generator;
pub mod grader_drafter;
pub mod training_monitor;
