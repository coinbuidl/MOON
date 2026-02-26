# MIP Future Considerations: Search Follow-ups

Status: Backlog (post-MIP-20260226)
Owner: TBD
Date: 2026-02-26

## Scope

These are follow-up options to consider **after** `mip20260226.md` (auto-embedding) lands and stabilizes.

## Option 2: Add Offline Relevance Evaluation

Goal:
1. Add a small, versioned offline eval set to measure retrieval quality.

Proposed metrics:
1. `hit@3`
2. `MRR`

Minimal contract:
1. Fixed query -> expected archive/projection IDs mapping.
2. Repeatable runner command.
3. CI-friendly output (pass/fail thresholds + trend logging).

## Option 3: Add Lightweight Reranker on Top-N QMD Hits

Goal:
1. Improve ranking precision without replacing current QMD backend.

Proposed shape:
1. Keep QMD as first-pass retrieval.
2. Rerank top `N` results (for example `N=20`) with a lightweight scoring stage.
3. Return top `K` after rerank.

Guardrails:
1. Deterministic ordering for score ties.
2. Configurable feature flag and budget limits.
3. Fallback to current ranking when reranker is unavailable.

## Option 4: Consider Backend Replacement (Only If Needed)

Goal:
1. Evaluate migration to another vector/search backend only if options 2/3 do not reach target quality/latency.

Decision gates:
1. Offline eval regression/plateau after embedding + reranker.
2. Operational cost and reliability targets not met.
3. Clear migration plan for index build, rollback, and data compatibility.

Potential candidates:
1. Qdrant
2. PostgreSQL + `pgvector`
3. Other managed/local vector stores as needed

## Recommended Sequencing

1. Ship and stabilize `mip20260226.md`.
2. Implement option 2 (measure first).
3. Implement option 3 (incremental improvement).
4. Revisit option 4 only with evidence from 2/3.
