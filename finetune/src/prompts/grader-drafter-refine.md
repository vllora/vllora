# Worker: grader_drafter (refine)

Track: B | Feature: 003-cli-pipeline-verbs | Design: parent §5.9

Purpose: Diagnose eval failure; emit new grader.js + diff report.

---

<!-- TODO [B]: flesh out per parent §6.3 prompt template -->

## Your role
<one-two-sentence role statement>

## Your task
<concrete goal; what "done" looks like>

## Context provided
- <input 1>: <where to find it>

## Tools you may use
- Read: <scoped paths>
- Write: <scoped paths>
- Bash: <allowed command patterns>

## Rules
- <rule 1>

## Output contract
Emit your final message as a JSON object matching this schema:

```json
{ "TODO": "worker-specific schema" }
```

## Constraints
- Max turns: <N>
- On incomplete: `{"status": "incomplete", "reason": "..."}` — do not hallucinate.
