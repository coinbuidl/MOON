# Technical Note: MOON Recall Failure Diagnosis & Workflow Analysis
**Case ID**: 20260221-0539-AEDT
**Author**: Lilac Metaflora
**Status**: Finalized for Improvement Plan

## 1. Executive Summary
During a precision retrieval task for a message sent at **2026-02-21 05:39 AEDT**, the MOON System's primary search (`moon-recall`) failed to locate the target. Subsequent manual forensic analysis of the raw `.jsonl` session logs successfully identified the data. This report outlines the technical reasons for the failure and provides architectural insights for the MOON Improvement Plan.

## 2. Failure Diagnosis (The "Why")

### A. Temporal Alignment & Timezone Sensitivity
- **Issue**: The query used "05:39 AEDT", while raw logs use UTC (`ISO 8601`).
- **Impact**: Standard lexical search fails because the string "05:39" does not exist in the timestamp field of the UTC log (`18:39Z`).
- **Recommendation**: MOON should implement a temporal normalization layer that converts user-provided local times into multiple standard formats (UTC, Epoch, YYYY-MM-DD) before querying.

### B. Signal-to-Noise Ratio in Projections
- **Issue**: The current projection logic (`archives/raw/*.md`) performs aggressive de-noising to save tokens.
- **Impact**: Critical anchors (like exact timestamps or tool call IDs) are often stripped. If the "Vector" search relies on these thin projections, it loses the "connective tissue" required for exact-match retrieval.
- **Recommendation**: Maintain a "High-Fidelity" index for metadata (timestamps, sender IDs) separate from the "Semantic" index for conversational content.

### C. Escaped JSON Complexity
- **Issue**: Tool results in `.jsonl` are often deeply nested and escaped (e.g., `\\\\"`).
- **Impact**: This creates "Token Noise" that confuses embedding models and breaks simple `grep` patterns.
- **Recommendation**: MOON's indexing pipeline should include a JSON-flattener/unescaper to normalize tool outputs before embedding.

## 3. Workflow Insights (The "Mechanism")

### A. The "Bricks vs. Wall" Architecture
- **Raw Logs (.jsonl)**: Store "Bricks" (incremental events, parent IDs, tool results).
- **In-Memory Prompt**: Represents the "Wall" (dynamically assembled context sent to API).
- **Implication**: MOON cannot simply "read the prompt" from the past; it must be able to *reconstruct* it by traversing the parent-child message tree.

### B. The Role of `Summary` (Memory Distillation)
- **Mechanism**: When context exceeds limits (e.g., 75k tokens), a compaction event occurs. An LLM-generated summary is injected into the next "Brick."
- **Current State**: Summary focuses on `Goal`, `Progress`, and `Decisions`.
- **MOON Opportunity**: Use these summaries as "Context Waypoints." If a search hits a summary, MOON should be triggered to explore the *raw* messages immediately following that summary's timestamp.

## 4. Proposed MOON Improvement Hooks
1. **Metadata-Aware Indexing**: Explicitly index `message_id` and `timestamp` as searchable fields, not just text blobs.
2. **Expansion Logic**: When a search hit has a score above X, automatically retrieve the `N` messages before and after via `parentId` traversal.
3. **Hybrid Search**: Combine lexical (BM25) for specific terms like "05:39" with Vector (Embedding) for semantic concepts.

---
*Generated for Master Brian's MOON Improvement Plan.*
*Location: ~/.lilac_metaflora/docs/moon_recall_failure_report_0539.md*
