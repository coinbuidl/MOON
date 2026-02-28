# MIP-LanceDB: Memory Vector Indexing (Deferred)

> **Status**: DEFERRED — extracted from MIP-20260228 for future evaluation.
> **Reason**: Dual-backend complexity (LanceDB + QMD) creates maintenance and debugging overhead that outweighs the benefit at this stage.

## 🎯 Objective
Add a Rust-native vector index for `memory/*.md` files using LanceDB, enabling semantic recall over daily logs and distilled insights.

## 🏗️ Architecture: LanceDB for Memory, QMD for Archives

LanceDB handles `memory/*.md` in-process. QMD continues to serve `archives/mlib/*.md`.

### Coexistence vs Full Replacement

| | LanceDB replaces QMD entirely | Coexistence |
|---|---|---|
| **Pros** | Single backend, simpler codebase, no Node/Bun dep, unified query | Lower risk, incremental rollout, no archive re-index |
| **Cons** | Larger change surface, archive re-index migration, higher blast radius | Two backends, two query paths, dual debugging |

### Data Sources (Phased)
- **Phase 1**: `memory/*.md` (daily logs and distilled insights).
- **Phase 2**: `MEMORY.md`, `USER.md`, `IDENTITY.md` (static knowledge files).
- **Unchanged**: `archives/mlib/*.md` remains on QMD.

### Embedding Pipeline
- **Model**: Flexible provider selection (mirror `MOON_DISTILL_PROVIDER` pattern):
    - `openai`: `text-embedding-3-small` / `text-embedding-3-large`
    - `local`: via `fastembed-rs` for zero-cost operation
- **Chunking**: Semantic paragraph-based, 15% overlap.
- **Metadata**: `file_path`, `date`, `checksum` (SHA-256 for skip-if-unchanged), `category`.

### Automated Sync
- Extend `moon-watch` to monitor `memory/` directory.
- Incremental re-indexing on file change (checksum-gated).
- Periodic deep sweep every 24 hours.

### Recall Interface
- `moon-recall --backend lancedb` alongside existing QMD recall.
- Merged results from both backends when querying across memory + archives.

## Pre-Requisites (Before Resuming)
1. [ ] Verify `lancedb` Rust SDK compiles and runs on Windows.
2. [ ] Evaluate if QMD can be fully replaced (removes dual-backend burden).
3. [ ] Assess API cost profile for embedding `memory/*.md` corpus.

## Resource Allocation
- **Disk Space**: Unrestricted.
- **Compute**: Background via `moon-watch` daemon.
- **Provider**: Flexible — OpenAI (default), local embeddings (zero-cost alt).

---
*Extracted from MIP-20260228 on 2026-02-28*
*Created by Lilac Livint | Master's Maid*
