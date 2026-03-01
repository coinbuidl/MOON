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
3. `WISDOM_DISTILL_FAILED`
4. `CONTINUITY_FAILED`
5. `RETENTION_DELETE_FAILED`
6. `LEDGER_READ_FAILED`
7. `INDEX_NOTE_FAILED`
8. `PROJECTION_WRITE_FAILED`
9. `DISTILL_SOURCE_MISSING`
10. `EMBED_FAILED`
11. `EMBED_LOCKED`
12. `EMBED_CAPABILITY_MISSING`
13. `EMBED_STATUS_FAILED`

## Warning Triage

1. `INDEX_FAILED`: verify `QMD_BIN`, `qmd collection add`, and `qmd update`.
2. `DISTILL_FAILED` / `WISDOM_DISTILL_FAILED`: verify distill provider credentials/model and source readability.
3. `CONTINUITY_FAILED`: verify session rollover command and continuity map write permissions.
4. `RETENTION_DELETE_FAILED`: verify archive file permissions and filesystem health.
5. `LEDGER_READ_FAILED`: verify `archives/ledger.jsonl` exists and contains valid JSONL records.
6. `INDEX_NOTE_FAILED`: verify gateway `chat.send` permissions and session key validity.
7. `PROJECTION_WRITE_FAILED`: verify archive read permissions and projection markdown write permissions.
8. `DISTILL_SOURCE_MISSING`: verify archive projection markdown exists (`archives/mlib/*.md`) and rerun `moon-index --name history` to backfill.
9. `EMBED_FAILED`: verify `QMD_BIN`, `qmd embed` execution, and file permissions for lock/state paths.
10. `EMBED_LOCKED`: another embed worker is active; retry next cycle or after current run ends.
11. `EMBED_CAPABILITY_MISSING`: installed QMD build lacks bounded embed capability (`--max-docs`); upgrade QMD.
12. `EMBED_STATUS_FAILED`: QMD embed returned failed status payload; inspect command output and QMD logs.

## Stage Policies

## Watcher Loop

Failure:
1. Config invalid.
2. State file read/write failure.

Policy:
1. Return non-zero from one-shot run.
2. In daemon mode, log and retry next cycle unless config is permanently invalid.
3. Panic guard wraps each cycle (`catch_unwind`): reset panic counter on any successful cycle; halt daemon after 3 consecutive panics (`DAEMON_PANIC_HALT`).
4. Corrupt state JSON auto-recovers by starting with defaults; best-effort corrupt backup is attempted first.

## Session Usage Provider

Failure:
1. OpenClaw metrics unavailable.

Policy:
1. Fail the cycle and surface a clear error when `openclaw` is unavailable (not on `PATH` and no valid `OPENCLAW_BIN` override).
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
1. Remote synthesis provider unavailable/timeout.
2. Parsing/output contract failure.

Policy:
1. Skip synthesis for this run and emit a degraded warning.
2. Ask user/operator to fix primary synthesis configuration (`MOON_WISDOM_PROVIDER`, `MOON_WISDOM_MODEL`, provider API key), then retry.

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

## Embed Stage

Failure:
1. QMD embed capability missing.
2. Active embed lock.
3. QMD embed command failed or reported failed status.

Policy:
1. Watcher mode: warn and continue cycle in degraded mode.
2. Manual mode: return `ok=false` on lock/capability/command failures (no degraded unbounded fallback flags).
3. Always append embed audit detail.

## CLI Workspace Boundary

Failure:
1. Mutating command executed outside expected workspace boundary.

Policy:
1. Return error with structured code `E004_CWD_INVALID`.
2. Diagnostic commands remain runnable from any directory.
3. Operator may bypass with global `--allow-out-of-bounds`.
