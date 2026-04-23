---
name: topic-hierarchy
description: |
  Design, validate, and iterate on the 2-level topic structure (Domain → Skill)
  that drives record generation. Covers skill-based vs document-based framing,
  difficulty distribution, enumerable-item policies, and when to merge/split.
  Triggers: "designing topics", "build topics", "merge topics", "split topics",
  "rebuilding topic structure", "per-topic eval scores", "difficulty weights",
  "topic coverage", "too many topics", "too few topics".
---

# Topic Hierarchy

## Overview

Topics organize training data by the **skill the model learns**, not by the shape of the source documents. A good 2-level tree (Domain → Skill, with difficulty as metadata) gives balanced coverage, enables targeted data generation, and allows per-skill eval analysis. Topic granularity is the largest source of run-to-run variance — get this right once and reuse the structure.

## Core rules

1. **Skill-based, never document-based.** A topic is a capability (`fork-detection`), not a chapter (`Chapter 3 Tactical Motifs`). Skill-based structures outperform content-mirror by ~33 points (STEPS, arXiv:2601.03676).
2. **Two levels only.** `Domain → Skill`. Difficulty is metadata on the leaf (`expected_difficulty: easy | medium | hard`), not a third level.
3. **Target ~15–25 records per leaf.** Below 15 → zero-variance collapse (arXiv:2509.21880). Above 25 → redundancy without breadth.
4. **Enumerable items get their own leaf each.** FDA Big-9 allergens, tax forms, product categories — one topic per item. Merging hides per-item difficulty; GRPO's largest gains come from the hardest items (47% from the hardest 10%, arXiv:2508.14094).
5. **Difficulty distribution targets 10–90% pass rate.** Hard (10–40% pass) ≈ 40–50% of records. Medium (40–70%) ≈ 30–40%. Easy (70–90%) ≈ 10–20%. <10% or >90% → zero gradient.
6. **Reuse existing topics.** If `topics.json` exists, treat as authoritative unless source material changed. Redesign is the biggest variance source.

## Anti-patterns

- **Mirroring document structure.** Skills scatter across chapters, overlaps appear, no difficulty control. Analyze what the material teaches, don't transcribe the TOC.
- **Too few topics (5 for 500 records).** 100 records/topic is redundancy. Split along skill lines + difficulty.
- **Merging enumerable items.** `Dairy+Egg` hides that Milk scores 0.8 and Egg scores 0.3. Keep them separate unless there are >40 items, then group by shared characteristics.
- **Per-document topics.** The same concept appearing in three PDFs becomes three duplicate topics. Synthesize into one skill topic linked to all three sources.
- **Missing `category` field.** Relations + `reconcile-topics` rely on `category: "single:milk"` or `"multi"`. Without it, name heuristics fail.

## Example — good (Chess tutor)

```
Tactical Pattern Recognition    (domain)
├── fork-detection              [hard]
├── pin-recognition             [medium]
└── combination-calculation     [hard]

Strategic Thinking              (domain)
├── pawn-structure-eval         [medium]
└── plan-formation              [hard]
```

## Example — bad (document mirror)

```
Chapter 3: Tactical Motifs
├── Section 3.1: Forks
├── Section 3.2: Pins
Chapter 5: Endgames
└── Section 5.1: King+Pawn
```

## When to split / merge

- **Split** a topic when per-topic eval shows bimodal scores (some records score 0.8, others 0.2 on the same "topic"). That's two sub-skills wearing one label.
- **Merge** only when two topics have overlapping records that score identically across iterations — pure redundancy.
- **Never merge** to hit a target topic count. Topic count is a consequence of skill coverage, not a lever.

## Diagnostic flow — eval shows a topic failing

1. Check per-topic score variance. Low variance on a low-score topic = the grader or the topic description is broken, not the model.
2. Check relations coverage: `relations.json` should map each leaf topic to ≥3 knowledge parts. Zero-relation topics produce empty record sets (`generate_records.py` skips them with a warning).
3. Check difficulty calibration — if `expected_difficulty: hard` but the topic scores 80%, relabel and re-allocate records.

## Research anchors

- STEPS skill-hierarchy superiority (arXiv:2601.03676).
- Zero-variance collapse below ~15 records/topic (arXiv:2509.21880).
- Hard-item gains dominance (arXiv:2508.14094 — 47% from hardest 10%).
- Schema-first prompting variance reduction (KONDA, SCI-K 2025).

## Related

- `grader-writing` — per-topic scoring patterns when topics have heterogeneous outputs.
- `readiness-gate` — soft check `topic_balance` flags skewed record counts.
- `pipeline-context` — `plan` phase is where topics are designed; `generate` allocates records per topic.
- Long-form: `vllora/ui/finetune-skill/reference/topic-hierarchy.md` (487 lines) has the full design rubric + per-type examples.
