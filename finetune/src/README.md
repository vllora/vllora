# `vllora_finetune` crate — existing client + new pipeline primitives

**Parent design:** `ui/docs/workflow-skill-first-approach/finetune-skill-command-redesign.md`

## What was here before

The `vllora_finetune` crate already provides the cloud HTTP client:
- `LangdbCloudFinetuneClient` (in `client.rs`) with 29 async methods.
- All finetune API types in `types.rs`.

## What was added

New modules that the CLI subcommand tree (`gateway/src/cli/commands/finetune/`) depends on:

```
src/
├── client.rs                 EXISTING — LangdbCloudFinetuneClient
├── types.rs                  EXISTING — 50+ types
├── lib.rs                    UPDATED — pub mod new modules below
│
├── state/                    NEW | Track A | Feature 002
│   ├── mod.rs                trait definitions (Journal, Analysis, ChangeLog, ExecutionLog)
│   ├── journal.rs            FileJournal + gateway sync via workflows.pipeline_journal
│   ├── analysis.rs           FileAnalysis + gateway sync via workflows.iteration_state
│   ├── change_log.rs         append-only markdown audit
│   ├── execution_log.rs      append-only decision cards
│   ├── atomic_write.rs       write-tmp-fsync-rename helper
│   ├── lock.rs               single-writer via fs2::FileExt
│   └── schemas/*.json        JSON Schema contracts
│
├── sources_adapters/         NEW | Track B | Feature 003
│   ├── mod.rs                SourceAdapter trait
│   ├── {local,hf,s3,gs,azblob,https}.rs
│
└── prompts/                  NEW | Track B | Feature 003
    └── *.md                  Worker system prompts (load via include_str!)
```

## To do

1. [ ] Add `pub mod state;` / `pub mod sources_adapters;` to `lib.rs`.
2. [ ] Add Cargo deps: `anyhow`, `async-trait`, `fs2`, `jsonschema`, `tokio`, `tokio-stream`.
3. [ ] Implement trait impls in `state/*.rs` modules.
4. [ ] Implement adapter impls in `sources_adapters/*.rs`.
5. [ ] Fill prompt content in `prompts/*.md` (migrate from `ui/finetune-skill/` where applicable).
