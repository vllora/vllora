# vllora-finetune — Claude Code plugin

Track: C
Feature: 004-claude-code-plugin
Parent design: `ui/docs/workflow-skill-first-approach/finetune-skill-command-redesign.md` (§2.3, §7)

## What this is

The Claude Code plugin bundle for vllora fine-tuning. Users invoke:

- `/finetune` — orchestrator (thick, stateful, dialogue-capable; parent §2.3.1)
- `/finetune-<verb>` — thin verb commands (one per phase; parent §2.3.2)

Plus auto-loaded reference skills under `skills/` for domain context.

**No pipeline logic lives here.** Every plugin command shells out to `vllora finetune <verb>` from the existing Rust CLI in `gateway/src/cli/`.

## How it reaches the user

At install time, `vllora init` (Feature 005) symlinks this directory into `~/.claude/plugins/vllora-finetune/`. The symlink is the pip-wheel's shipped artifact.

## Structure

```
plugin/
├── plugin.json                    Claude Code manifest
├── commands/                      1 orchestrator + 9 thin verb commands
│   ├── finetune.md                orchestrator — /finetune
│   ├── finetune-quickstart.md     thin — /finetune-quickstart
│   ├── finetune-init.md
│   ├── finetune-sources.md
│   ├── finetune-import-dataset.md
│   ├── finetune-plan.md
│   ├── finetune-generate.md
│   ├── finetune-eval.md
│   ├── finetune-train.md
│   └── finetune-status.md
├── skills/                        5 reference skills (auto-loaded)
│   ├── pipeline-context/SKILL.md
│   ├── grader-writing/SKILL.md    migrate from ui/finetune-skill/reference/
│   ├── topic-hierarchy/SKILL.md   migrate from ui/finetune-skill/reference/
│   ├── readiness-gate/SKILL.md    migrate from ui/finetune-skill/reference/
│   └── nemo-guide/SKILL.md        migrate from ui/finetune-skill/reference/
└── resources/
    ├── templates/                 .gitkeep — populate as needed
    └── reference/                 .gitkeep — long-form docs if needed
```

## Status

Stubs. See `ui/docs/workflow-skill-first-approach/gap-analysis.md` for what's implemented elsewhere vs what this plugin adds.

TODO [Track C / Feature 004]:
1. Flesh out orchestrator playbook per parent §7.3.1 template.
2. Flesh out 9 thin verb commands per parent §7.3.2 template.
3. Migrate reference-skill content from `ui/finetune-skill/reference/` with proper frontmatter descriptions for auto-load.
4. Verify `plugin.json` shape against current Claude Code plugin loader (parent §11 Q3).
