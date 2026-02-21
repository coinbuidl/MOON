# MIP-260221-SUPPLEMENT: Additional Architectural Refinements
**Date**: 2026-02-21 AEDT
**Author**: Lilac Metaflora
**Reference**: [MIP260221.md](./MIP260221.md)
**Status**: Recommendation for Integration

---

## 1. Compaction Traceability (Summary Anchors)
To ensure summaries act as functional "wormholes" back to raw history rather than just static text blocks.

- **Feature**: Store the specific `message_id` associated with each compaction event.
- **Logic**: In `ProjectionData`, map `compaction_notes` to their origin `message_id`.
- **Benefit**: If a search hit occurs on a summary, the system can instantly calculate the exact message offset in the source JSONL to begin deep-traversal reconstruction.

## 2. Side-Effect Prioritization (Actionable Signals)
Differentiating between passive observation (reading) and active modification (writing/executing).

- **Feature**: Semantic tagging of `tool_calls` based on their side-effect potential.
- **Logic**: 
    - **High Priority**: `write_to_file`, `exec (git/rm/mv)`, `edit`, `gateway (config.patch)`.
    - **Normal Priority**: `read_file`, `web_search`, `ls`.
- **Benefit**: Boosts the ranking of "Modification Events" in the timeline. When searching for "When did we change the config?", the system will favor write-tool entries over read-tool entries.

## 3. Contextual Stitching (Thread Coupling)
Preserving the "Cause and Effect" of tool usage within the projection.

- **Feature**: Couple `assistant:toolUse` and `tool:toolResult` entries in the `## Tool Activity` section.
- **Logic**: Instead of listing them as separate timeline events, group them as single "Transactions" in the projection.
- **Benefit**: Reduces fragmentation. The vector embedding for a tool transaction will capture both the *intent* (the command) and the *outcome* (the result) in a single high-signal block.

## 4. Multi-Format Timestamp Injection
Exposing more "surface area" for temporal queries.

- **Feature**: Inject "Natural Language" timestamps into the projection body.
- **Logic**: Instead of just `18:39:12Z`, periodically inject strings like `[Saturday Morning]`, `[Yesterday]`, or `[2 hours into session]` into the timeline.
- **Benefit**: Improves recall for vague temporal queries (e.g., "What did we do last Saturday morning?") which vector models handle better than raw ISO strings.

---
*Supplementary notes for Master Brian's MOON Improvement Plan.*
