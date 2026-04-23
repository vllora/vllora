//! Static tests for the Claude Code plugin bundle.
//!
//! Track: C | Feature: 004-claude-code-plugin
//! Design: `specs/004-claude-code-plugin/spec.md` FR-003, FR-005, FR-006, FR-008.
//!
//! These tests run without Track B's verb implementations. They assert
//! filesystem-level invariants:
//!   - Every plugin thin verb maps 1:1 to a `FinetuneCommand` variant.
//!   - Every command / skill file is ≤150 lines (FR-008).
//!   - No plugin file contains executable-code fences (FR-005).
//!   - Every thin verb shells to the matching `vllora finetune <verb>`.
//!
//! Behavioural tests (that actually run the plugin path) live in
//! `gateway/tests/plugin_behavior.rs` and use the `mock_vllora` binary.

#![cfg(test)]

use std::path::PathBuf;

/// Absolute path to `<repo>/plugin/` at test time. `CARGO_MANIFEST_DIR` is the
/// gateway crate root; its parent is the workspace root that holds `plugin/`.
fn plugin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gateway/ must have a parent")
        .join("plugin")
}

/// The 9 thin verbs the plugin MUST expose, per spec 004 FR-003.
///
/// Not all `FinetuneCommand` variants are plugin-surfaced:
///   - `Jobs` is Layer B (terminal-only, not a plugin command).
///   - `Auto` is the autonomous loop (terminal-only — users who want hands-off
///     execution run it from the shell, not via a slash command, because it
///     needs pre-flight setup the plugin can't provide interactively).
const EXPECTED_THIN_VERBS: &[&str] = &[
    "init",
    "sources",
    "import-dataset",
    "plan",
    "generate",
    "eval",
    "train",
    "status",
    "quickstart",
];

/// The 5 reference skills per spec 004 FR-004.
const EXPECTED_SKILLS: &[&str] = &[
    "pipeline-context",
    "grader-writing",
    "topic-hierarchy",
    "readiness-gate",
    "nemo-guide",
];

/// FR-008: every plugin file stays under this line count.
const MAX_LINES: usize = 150;

#[test]
fn orchestrator_file_exists() {
    let orchestrator = plugin_dir().join("commands").join("finetune.md");
    assert!(
        orchestrator.is_file(),
        "missing orchestrator at {}",
        orchestrator.display()
    );
}

#[test]
fn every_thin_verb_has_a_matching_command_file() {
    for verb in EXPECTED_THIN_VERBS {
        let expected = plugin_dir()
            .join("commands")
            .join(format!("finetune-{}.md", verb));
        assert!(
            expected.is_file(),
            "missing plugin command for verb '{}' at {}. \
             FinetuneCommand → plugin parity is a Feature 004 FR-003 invariant.",
            verb,
            expected.display()
        );
    }
}

#[test]
fn every_expected_skill_exists() {
    for slug in EXPECTED_SKILLS {
        let expected = plugin_dir().join("skills").join(slug).join("SKILL.md");
        assert!(
            expected.is_file(),
            "missing reference skill '{}' at {}",
            slug,
            expected.display()
        );
    }
}

#[test]
fn no_plugin_file_exceeds_max_lines() {
    for path in walk_markdown(&plugin_dir()) {
        let content = std::fs::read_to_string(&path).expect("readable");
        let lines = content.lines().count();
        assert!(
            lines <= MAX_LINES,
            "{} has {} lines (limit {}). Move long-form content into plugin/resources/reference/.",
            path.display(),
            lines,
            MAX_LINES
        );
    }
}

#[test]
fn no_forbidden_code_fences() {
    // Spec 004 FR-005 applies to COMMANDS specifically: "Every plugin command
    // MUST contain zero executable code." Skills are reference content and may
    // legitimately show illustrative code (e.g., `grader-writing` teaches JS
    // grader patterns — blocking ```javascript there would defeat the skill's
    // purpose). This test scans `plugin/commands/**/*.md` only.
    const FORBIDDEN_TAGS: &[&str] = &["python", "rust", "javascript", "js", "ts", "typescript", "bash-script"];

    let commands_dir = plugin_dir().join("commands");
    for path in walk_markdown(&commands_dir) {
        let content = std::fs::read_to_string(&path).expect("readable");
        for (lineno, line) in content.lines().enumerate() {
            let stripped = line.trim();
            if let Some(rest) = stripped.strip_prefix("```") {
                let tag = rest.trim().to_ascii_lowercase();
                if FORBIDDEN_TAGS.iter().any(|f| &tag == f) {
                    panic!(
                        "{}:{} uses forbidden code fence ```{}```. Plugin commands must contain no executable code (spec 004 FR-005).",
                        path.display(),
                        lineno + 1,
                        tag
                    );
                }
            }
        }
    }
}

#[test]
fn thin_verbs_shell_to_matching_cli_verb() {
    // Each thin verb .md MUST mention `vllora finetune <verb>` somewhere —
    // the Bash invocation it shells out to. Regression test for FR-006.
    for verb in EXPECTED_THIN_VERBS {
        let path = plugin_dir()
            .join("commands")
            .join(format!("finetune-{}.md", verb));
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));
        let expected_call = format!("vllora finetune {}", verb);
        assert!(
            content.contains(&expected_call),
            "{} does not mention `{}`. Thin verbs MUST shell to their matching CLI verb (FR-006).",
            path.display(),
            expected_call
        );
    }
}

/// Reverse-direction parity: the `FinetuneCommand` enum's Layer A variants
/// must ALL be accounted for — either plugin-surfaced (in
/// `EXPECTED_THIN_VERBS`) or explicitly terminal-only (in
/// `EXPECTED_TERMINAL_ONLY`). Adding a new Layer A verb without touching this
/// list fails the test — forcing a deliberate choice about plugin surfacing.
///
/// We drive the check off the source text of `mod.rs` to avoid pulling clap's
/// reflection in as a test dep. This is brittle to formatting but intentional:
/// it surfaces whenever a new verb is added.
#[test]
fn every_cli_verb_is_plugin_surfaced_or_explicitly_terminal_only() {
    // Terminal-only Layer A verbs — intentionally NOT plugin-surfaced. Keep
    // this in sync with the comment block at EXPECTED_THIN_VERBS above.
    const EXPECTED_TERMINAL_ONLY: &[&str] = &[
        "auto", // autonomous loop; needs pre-flight shell setup the plugin can't provide.
    ];
    // Layer B lives under the `Jobs` subcommand and is explicitly terminal-
    // only (§2.4 of the redesign). `Jobs` is NOT a Layer A verb, so we skip
    // it from the parity check — represented by its own `JobsCommand` enum.
    const SKIP_VARIANTS: &[&str] = &["Jobs"];

    let mod_rs = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/cli/commands/finetune/mod.rs"),
    )
    .expect("mod.rs readable");

    // Parse clap variant lines like:  `ImportDataset(import_dataset::Args),`
    // preceded by an optional `#[command(name = "import-dataset")]` line that
    // overrides the derived name.
    let mut variants: Vec<String> = Vec::new();
    let mut next_name_override: Option<String> = None;
    let mut in_enum = false;
    for line in mod_rs.lines() {
        let t = line.trim();
        if t.starts_with("pub enum FinetuneCommand") {
            in_enum = true;
            continue;
        }
        if !in_enum {
            continue;
        }
        if t == "}" {
            break;
        }
        if let Some(rest) = t.strip_prefix("#[command(name = \"") {
            if let Some(end) = rest.find('"') {
                next_name_override = Some(rest[..end].to_string());
            }
            continue;
        }
        // Skip any other attribute lines (`#[command(subcommand)]`, etc.).
        if t.starts_with("#[") {
            continue;
        }
        // A variant line looks like `Init(init::Args),` — grab the text
        // before the first `(`.
        if let Some(paren) = t.find('(') {
            let variant = &t[..paren];
            if variant.is_empty() || variant.starts_with("//") {
                continue;
            }
            let cli_name = next_name_override
                .take()
                .unwrap_or_else(|| pascal_to_kebab(variant));
            variants.push(cli_name);
        }
    }

    assert!(
        !variants.is_empty(),
        "parser failed to find any FinetuneCommand variants — test is broken"
    );

    for cli_name in &variants {
        if SKIP_VARIANTS.iter().any(|s| pascal_to_kebab(s) == *cli_name) {
            continue;
        }
        let surfaced = EXPECTED_THIN_VERBS.contains(&cli_name.as_str());
        let terminal_only = EXPECTED_TERMINAL_ONLY.contains(&cli_name.as_str());
        assert!(
            surfaced || terminal_only,
            "CLI verb `{cli_name}` is neither plugin-surfaced nor in \
             EXPECTED_TERMINAL_ONLY. Add it to one of those lists (and write \
             the plugin command if you want it surfaced)."
        );
    }
}

fn pascal_to_kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[test]
fn plugin_files_do_not_read_credentials() {
    // FR-013: plugin never reads ~/.claude/.credentials.json or the env var.
    // The test targets the `Read` tool pointed at these paths — a mere prose
    // mention ("never read .credentials.json") is fine. Heuristic: look for
    // the forbidden path immediately after a tool-invocation verb like
    // `Read(`, `cat `, or `open(`. Scans commands only (skills are reference
    // content; they can cite credential paths in prose).
    const FORBIDDEN_READ_PATTERNS: &[&str] = &[
        "Read(~/.claude/.credentials",
        "Read('~/.claude/.credentials",
        "Read(\"~/.claude/.credentials",
        "cat ~/.claude/.credentials",
        "cat ~/.claude/credentials",
        "open(~/.claude/.credentials",
        "fs.read('~/.claude",
        "fs.readFile('~/.claude",
    ];

    let commands_dir = plugin_dir().join("commands");
    for path in walk_markdown(&commands_dir) {
        let content = std::fs::read_to_string(&path).expect("readable");
        for pat in FORBIDDEN_READ_PATTERNS {
            assert!(
                !content.contains(pat),
                "{} contains forbidden credential-read pattern `{}`. Auth flows through `claude -p` subprocess only (parent §2.10.1).",
                path.display(),
                pat
            );
        }
    }
}

/// Walk every `.md` file under `dir` recursively.
fn walk_markdown(dir: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_into(dir, &mut out);
    out
}

fn walk_into(dir: &PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_into(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
            out.push(path);
        }
    }
}
