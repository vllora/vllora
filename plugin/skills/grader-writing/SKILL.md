---
name: grader-writing
description: |
  Design, diagnose, and fix `grader.js` — the JavaScript scoring function
  GRPO uses as a reward signal. Covers rubric-based scoring, anti-gaming
  defenses, DRPO-safe length penalties, LLM-as-judge patterns, and the
  common failure modes that produce zero-gradient training.
  Triggers: "writing a grader", "fix grader", "grader returns 0", "reward hacking",
  "score clustering", "LLM-as-judge", "response length is growing", "model is padding".
---

# Grader Writing

## Overview

The grader is a synchronous JavaScript function evaluated in a QuickJS sandbox. It returns `{score: 0-1, reason: string}` for every candidate response. GRPO uses the score as its reward signal — the grader IS the training objective. Bad graders burn GPU hours; good graders with rubric scoring + anti-gaming defenses teach the intended behaviour.

## Core patterns

1. **Function contract.** Must expose `evaluate(input)` returning `{score, reason}` synchronously. Input has `input.messages` (full conversation), `input.response` (assistant output), `input.history`, `input.ground_truth`.
2. **Three families to pick from.**
   - *Pure programmatic* — best for structured outputs (JSON, tool calls, enum labels).
   - *LLM-as-judge* — subjective quality assessment via `__langdb_call_llm_as_judge_obj`.
   - *Hybrid* — hard programmatic gate first, then LLM quality scoring on top.
3. **Rubric decomposition beats holistic 0–5.** Design 7–20 binary criteria instead of one scale. arXiv:2507.17746 reports +31% over holistic. Categorize: Essential (weight 1.0), Important (0.7), Optional (0.3).
4. **Stratify correctness before quality.** `correctness=true` scores 0.5–1.0; `correctness=false` scores 0.0–0.5. Prevents well-written-but-wrong from out-scoring correct-but-terse.
5. **DRPO-safe length penalty (arXiv:2510.04474).** Never apply uniformly. Apply only to wrong/partial answers. Correct answers get a brevity *bonus* (+0.03–0.05). Multiplicative penalty: `score *= min(1.0, targetTokens / actualTokens)`.

## Anti-patterns (block on review)

- **"Return 0.0 when parsing fails."** FALSE. A 0.0 means genuinely wrong — parsing failures waste eval runs with zero gradient. Use an LLM-extraction fallback, or a neutral floor (0.02 for tool-calling wrong-tool).
- **"Write the grader from scratch."** Don't. Copy a template (`grader-mcq.js`, `grader-classification.js`, multiplicative ToolRLA). Hand-rolled scorers produce coarse 0/1 clustering → GRPO gets no gradient.
- **"Binary pass/fail only."** Weak signal. Partial credit across 0–1 gives the strongest learning; the model learns most from 0.4–0.7 scores, not the 0/1 extremes.
- **"Skip exploitation testing."** Adversarial test every grader: correct+padding, wrong+format compliance, prompt echoing. If any exploit scores > 0.4, GRPO WILL find it.
- **"Same judge as training model."** Increases in-context reward hacking. Use a different model family (e.g., GPT-4.1 judges a Qwen model).
- **"Hard 0.0 floor on wrong answers (tool-calling)."** Causes ≥80% zero-variance batches → flat training. Use 0.02 minimum (ToolRL convention).

## Template snippets

LLM-as-judge minimal shape:

```javascript
const result = __langdb_call_llm_as_judge_obj(config, input);
// config = {prompt_template: [{role, content}, ...], output_schema: {...}, completion_params}
// result matches the schema, OR {error: "..."}
const finalScore = (result.accuracy + result.helpfulness) / 2 / 5;
return { score: finalScore, reason: result.reasoning };
```

DRPO-safe length policy (bonus for correct, penalty for wrong):

```javascript
if (verdict === 'CORRECT') {
  const bonus = actualTokens <= 80 ? 0.03 : 0.0;
  return Math.min(1.0, baseScore + bonus);
}
// wrong/partial — penalty, with 0.02 floor
const lengthPenalty = Math.min(1.0, 80 / actualTokens);
return Math.max(0.02, baseScore * lengthPenalty);
```

## Diagnostic flow when scores cluster

If eval scores bunch at one value (e.g., 79% of records score 0.3):
1. **Suspect the grader first, not the data.** Score clustering is almost always a grader problem.
2. Check `grader-sanity-check` output for category distribution.
3. Look for `countOOVSegments` / `countDuplicateEmissions` in the grader — missing these is a common reward-hacking surface.
4. If the cluster is at 0.0, check for hard-zero return paths (parsing failures, refusal detection) — replace with 0.02 floor or LLM fallback.

## Research anchors

- Rubrics as Rewards (arXiv:2507.17746) — +31% over holistic.
- Rethinking Rubric Design (arXiv:2602.05125).
- DRPO length penalty (arXiv:2510.04474).
- Dr. GRPO / DAPO / GR3 — length-exploitation defenses (arXiv:2503.20783, arXiv:2503.14476, arXiv:2603.10535).

## Related

- `readiness-gate` — the post-eval checks that catch score_concentration > 70% (grader broken).
- `pipeline-context` — phase ordering; the `generate` phase's finalize mode locks in the grader.
- Long-form: `vllora/ui/finetune-skill/reference/grader-writing.md` (943 lines) holds the full playbook with worked examples per grader family.
