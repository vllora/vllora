---
name: readiness-gate
description: |
  Interpret the two post-eval validation checks that gate training:
  the readiness gate (aggregate stats) and the difficulty probe (per-prompt
  signal). Covers hard/soft checks, thresholds, root-cause routing
  (grader vs data vs topic), and the headroom gate.
  Triggers: "before training, run readiness check", "is my data ready",
  "low score variance", "too many zero scores", "zero-variance prompts",
  "difficulty probe", "grader is too strict", "grader is too lenient",
  "eval just finished".
---

# Readiness Gate

## Overview

Two cheap validation checks run after `/finetune-eval` to catch failures that would waste GPU hours. The **readiness gate** checks aggregate statistics (sample count, score std, zero-score fraction). The **difficulty probe** checks per-prompt signal strength at K=8 (catches data that looks good in aggregate but produces zero gradient). Both consume the eval results you already have — no additional inference.

## Hard checks (all must pass to train)

- **Sample count ≥ 50.**
- **Score std > 0.10.** Below = grader produces one value; GRPO has no gradient.
- **Average score > 0.05.** Below = zero headroom; the base model is fundamentally unable on this task (GRPO cannot create ability, arXiv:2504.03380).
- **Exact-zero fraction < 10%.** More than 10% hard zeros usually means a broken grader floor — use 0.02 instead.

If any hard check fails, training is blocked. Fix the specified problem before re-evaluating.

## Soft checks (warnings only, except one)

- `score_concentration < 50%` — warning at 50%, **hard fail at 70%**. Concentration > 70% means most prompts score identically → most K=8 groups score identically → zero gradient. **Fix grader, don't train through.**
- `binary_frac < 60%` — warning. Fine to train through; DeepSeek-R1 + DAPO both trained on 100% binary rewards successfully (arXiv:2501.12948, arXiv:2503.14476).
- `dead_weight < 50%` — warning. Dead-weight prompts (always right or always wrong in K=8) carry no gradient; 30–99% is normal in GRPO.
- `topic_balance` — warning if any topic has fewer than the minimum.

## Headroom gate (hard bound)

Base-model average score must be in **[0.05, 0.75]**.
- `< 0.05`: no latent capability. Try a larger base model or add SFT warmup.
- `> 0.75`: near-zero gradient (most groups are already all-correct). Too-easy data — add harder records via `harden-records`.
- **Optimal zone: 0.30–0.70** (arXiv:2504.03380 Table 1).

## Difficulty probe (catches per-prompt failure modes)

Even if the readiness gate passes, run the difficulty probe:

```bash
vllora finetune grader dryrun --difficulty-probe --file eval-001.json
```

Exit codes:
- **0 (PASS)**: ≥30% learnable AND grader is granular.
- **2 (WARN)**: 15–30% learnable OR high zero-variance. Review recommendations.
- **1 (FAIL)**: <15% learnable OR score_concentration > 70%. Don't train.

**Learnable** = prompts where K=8 samples produce mixed outcomes (some correct, some wrong). Maximum gradient at `p=0.5`. DOTS+RR (arXiv:2506.05316) confirms learnable prompts drive almost all learning.

## Root-cause routing (when eval fails)

Three root causes, each with a distinct remediation:

1. **GRADER root cause** — score_concentration > 70%, or scores cluster at the floor/ceiling. The CLI auto-spawns `grader_drafter(refine)`. Surface the `change-log.md` tail + re-eval.
2. **DATA root cause** — trivial_frac > 40% with learnable < 35%, or dead_weight > 60% with unambiguous grader. Don't refine the grader; revisit `/finetune-plan` or `/finetune-generate` and diversify records.
3. **TOPIC root cause** — a specific leaf topic hits zero-variance while others are healthy. Split the topic (bimodal outcomes = two sub-skills), or add per-topic records.

## Trivial-record threshold

If `>40% of records score >0.95` AND `learnable% < 35%` → apply `harden-records` to inject harder variants alongside the originals. Keep if learnable% already healthy.

## Anti-patterns

- **"Train despite score_concentration > 70%"** — NO. Grader is broken; wasted GPU guaranteed.
- **"Expect high base-model scores"** — false. 10–40% pass rate is exactly where GRPO gains are largest.
- **"High binary_frac = broken data"** — false. Mixed outcomes *within K=8 groups* is what matters, not the absolute reward shape.
- **"Skip difficulty probe because readiness passed"** — no. Readiness checks aggregates; the probe checks per-prompt signal. Data can pass readiness and still have >70% zero-variance prompts.

## Research anchors

- DeepSeek-R1 base → trained gains (arXiv:2501.12948): 15.6% → 71%.
- Headroom zone 0.30–0.70 optimal (arXiv:2504.03380).
- Learnable-prompt signal (arXiv:2506.05316 DOTS+RR).
- Hardest-item gains (arXiv:2508.14094): 47% gain from hardest 10%.

## Related

- `grader-writing` — if root cause is GRADER, that's the rubric reference.
- `topic-hierarchy` — if root cause is TOPIC, splitting/merging lives there.
- `pipeline-context` — where readiness fits in the phase loop.
- Long-form: `vllora/ui/finetune-skill/reference/readiness-gate.md` (300 lines) has the full check table + decision trees.
