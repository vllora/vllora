# `vllora finetune <verb>` subcommand tree

**Track B** | **Feature 003-cli-pipeline-verbs** | **Parent design:** `ui/docs/workflow-skill-first-approach/finetune-skill-command-redesign.md`

## What this is

The new CLI subcommand tree implementing the pipeline verbs. Wires into the existing `Commands` enum in `gateway/src/cli/mod.rs`:

```rust
pub enum Commands {
    Serve(ServeArgs),
    List,
    Sync { models, providers },
    Traces(TracesCommands),
    GenerateModelsJson { output },

    // NEW — added by Feature 003:
    Finetune(commands::finetune::FinetuneCommand),
}
```

Dispatch in `gateway/src/main.rs` match block:

```rust
Some(cli::Commands::Finetune(cmd)) => cmd.run(db_pool).await,
```

## Structure

```
finetune/
├── mod.rs                    FinetuneCommand enum
├── {verb}.rs                 handle_{verb}(args, db_pool) per pipeline verb
├── jobs/                     Layer B direct-job wrappers (Feature 001)
└── workers/                  claude -p subprocess management
```

Each verb's handler composes:
- Existing `vllora_finetune::LangdbCloudFinetuneClient` methods for cloud ops
- `finetune::state::*` for local state files + gateway journal sync
- `finetune::sources_adapters::*` for URI resolution (sources verb)
- `workers::*` for `claude -p` subprocesses (LLM-heavy steps)
- Existing Python scripts in `ui/finetune-skill/scripts/` via `tokio::process::Command`

## Wire-up checklist (Week 1)

1. [ ] Add `Finetune(FinetuneCommand)` variant to `Commands` enum in `gateway/src/cli/mod.rs`.
2. [ ] Add dispatch arm in `gateway/src/main.rs`.
3. [ ] Add dependency on `vllora_finetune = { path = "../finetune" }` in `gateway/Cargo.toml`.
4. [ ] Implement each verb's `handle` function.
5. [ ] Test `vllora finetune status` works end-to-end on a sample project.
