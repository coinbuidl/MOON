# moon System Contracts

## Scope

This document defines Phase 0 contract shapes for the moon watcher pipeline.

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
3. `provider: String` (for example `l1-normaliser`, `local`, `openai`, `anthropic`, `gemini`, `openai-compatible`)
4. `summary_path: String`
5. `audit_log_path: String`
6. `created_at_epoch_secs: u64`

Rules:
1. Must always include `provider` and output paths.
2. Failure path must emit an audit record.

## Distill Trigger Contract

Fields:
1. `watcher.cooldown_secs: u64`
2. `distill.max_per_cycle: u64`
3. `distill.residential_timezone: String` (IANA TZ; default `UTC`)
4. `distill.chunk_bytes: Option<String>` (`"auto"` or positive integer)
5. `distill.max_chunks: Option<u64>` (`>= 1` when set)
6. `distill.model_context_tokens: Option<u64>` (`>= 1` when set)

Rules:
1. Watcher auto L1 runs only when cooldown passes and pending projection markdown exists in `archives/mlib/*.md`.
2. L1 selection is deterministic and bounded by `max_per_cycle`.
3. L1 lock is non-blocking and single-run; watcher lock contention degrades/skips current cycle.
4. `moon-watch --once --distill-now` is a manual watcher trigger for the same L1 queue.
5. Manual L1 (`moon distill -mode norm -archive <path>`) requires explicit readable pending projection path and lock availability; lock/no-pending returns error.
6. Auto L2 Synthesis (`syns`) runs once per residential day on the first watcher cycle after local midnight.
7. Auto `syns` sources are yesterday's daily memory file plus current `memory.md` (when present).
8. Direct CLI triggers: `moon distill -mode norm` (L1 Normalisation) and `moon distill -mode syns` (L2 Synthesis).

## DaemonLockPayload

Fields:
1. `pid: u32`
2. `started_at_epoch_secs: u64`
3. `build_uuid: String`
4. `moon_home: String`

Rules:
1. Lock file path is `$MOON_LOGS_DIR/moon-watch.daemon.lock`.
2. Payload is JSON; legacy single-line PID lock payloads remain backward compatible for readers.
3. Mutating commands may use `moon_home` to enforce workspace boundary checks.

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
