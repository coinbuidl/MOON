# Moon System Contracts

## Scope

This document defines Phase 0 contract shapes for the Moon watcher pipeline.

## SessionUsageSnapshot

Fields:
1. `session_id: String`
2. `used_tokens: u64`
3. `max_tokens: u64`
4. `usage_ratio: f64` (`used_tokens / max_tokens`)
5. `captured_at_epoch_secs: u64`
6. `provider: String`

Rules:
1. `max_tokens > 0`
2. `usage_ratio` in `[0.0, 1.0+]`

## ArchiveRecord

Fields:
1. `session_id: String`
2. `archive_path: String`
3. `content_hash: String`
4. `created_at_epoch_secs: u64`
5. `indexed_collection: String`
6. `projection_filtered_noise_count: Option<usize>`

Rules:
1. `content_hash` is deterministic for identical snapshots.
2. Same hash + session pair is idempotent.
3. `projection_filtered_noise_count` records deterministic pre-emptive noise filtering volume when projection markdown is generated.

## DistillationRecord

Fields:
1. `session_id: String`
2. `archive_path: String`
3. `provider: String` (`local` or `gemini-2.5-flash-lite`)
4. `summary_path: String`
5. `audit_log_path: String`
6. `created_at_epoch_secs: u64`

Rules:
1. Must always include `provider` and output paths.
2. Failure path must emit an audit record.

## Distill Trigger Contract

Fields:
1. `distill.mode: String` (`manual`, `idle`, or `daily`)
2. `distill.idle_secs: u64`
3. `distill.max_per_cycle: u64`
4. `distill.residential_timezone: String` (IANA TZ; default `UTC`)

Rules:
1. `idle` mode starts only after the latest archive has been idle for `idle_secs`.
2. Selection is deterministic: oldest pending archive day first, then up to `max_per_cycle`.
3. `daily` mode attempts layer-2 distillation once per residential day after the latest archive has been idle for `idle_secs`.
4. `manual` mode disables automatic layer-2 distillation; manual trigger is `moon-watch --once --distill-now`.

## ContinuityMap

Fields:
1. `source_session_id: String`
2. `target_session_id: String`
3. `archive_refs: Vec<String>`
4. `daily_memory_refs: Vec<String>`
5. `key_decisions: Vec<String>`
6. `generated_at_epoch_secs: u64`

Rules:
1. Must be deterministic and machine-readable.
2. Must include at least one archive reference.

## RecallResult

Fields:
1. `query: String`
2. `matches: Vec<RecallMatch>`
3. `generated_at_epoch_secs: u64`

`RecallMatch` fields:
1. `archive_path: String`
2. `snippet: String`
3. `score: f64`
4. `metadata: serde_json::Value`

Rules:
1. Output must be safe to inject into active session context.
2. Include ranking score for deterministic ordering.
