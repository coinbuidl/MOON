# moon System Failure Policy

## Principles

1. Archive before any destructive reduction.
2. Prefer degraded operation over hard stop.
3. Always emit audit detail for failures and fallbacks.
4. Emit AI-readable warning lines for actionable failures:
`MOON_WARN code=<CODE> stage=<STAGE> action=<ACTION> session=<SESSION_ID> archive=<ARCHIVE_PATH> source=<SOURCE_PATH> retry=<RETRY_POLICY> reason=<REASON> err=<ERR_SUMMARY>`.

## Warning Codes

1. `INDEX_FAILED`
2. `DISTILL_FAILED`
3. `DISTILL_CHUNKED_FAILED`
4. `CONTINUITY_FAILED`
5. `RETENTION_DELETE_FAILED`
6. `LEDGER_READ_FAILED`
7. `INDEX_NOTE_FAILED`
8. `PROJECTION_WRITE_FAILED`
9. `DISTILL_SOURCE_MISSING`

## Warning Triage

1. `INDEX_FAILED`: verify `QMD_BIN`, `qmd collection add`, and `qmd update`.
2. `DISTILL_FAILED` / `DISTILL_CHUNKED_FAILED`: verify distill provider credentials/model and archive readability.
3. `CONTINUITY_FAILED`: verify session rollover command and continuity map write permissions.
4. `RETENTION_DELETE_FAILED`: verify archive file permissions and filesystem health.
5. `LEDGER_READ_FAILED`: verify `archives/ledger.jsonl` exists and contains valid JSONL records.
6. `INDEX_NOTE_FAILED`: verify gateway `chat.send` permissions and session key validity.
7. `PROJECTION_WRITE_FAILED`: verify archive read permissions and projection markdown write permissions.
8. `DISTILL_SOURCE_MISSING`: verify archive projection markdown exists (`archives/mlib/*.md`) and rerun `moon-index --name history` to backfill.

## Stage Policies

## Watcher Loop

Failure:
1. Config invalid.
2. State file read/write failure.

Policy:
1. Return non-zero from one-shot run.
2. In daemon mode, log and retry next cycle unless config is permanently invalid.

## Session Usage Provider

Failure:
1. OpenClaw metrics unavailable.

Policy:
1. Fail the cycle and surface a clear error (`OPENCLAW_BIN` required and executable).
2. Retry next cycle in daemon mode after normal poll interval.

## Archive Stage

Failure:
1. Source session missing.
2. Archive write failure.

Policy:
1. Hard stop downstream compaction/distill for this cycle.
2. Retry next cycle after cooldown.

## QMD Index Stage

Failure:
1. QMD binary missing.
2. `qmd collection add/search` non-zero exit.

Policy:
1. Mark archive as unindexed in ledger.
2. Allow retry queue in later cycles.
3. Do not continue to destructive stages if no archive reference is available.

## Compaction Stage

Failure:
1. Plugin action fails.

Policy:
1. Keep current session unchanged.
2. Continue monitoring; no forced rollover.

## Distill Stage

Failure:
1. Gemini API unavailable/timeout.
2. Parsing/output contract failure.

Policy:
1. Fallback to local distiller.
2. If local also fails, skip continuity rollover and keep archive for recall.

## Continuity/Rollover Stage

Failure:
1. New session creation failure.
2. Semantic map injection failure.

Policy:
1. Keep old session active.
2. Record failure in audit log and retry on next qualifying cycle.

## Recall Stage

Failure:
1. QMD search failure.
2. No matches.

Policy:
1. Return structured empty result.
2. Never fail hard on no-match conditions.
