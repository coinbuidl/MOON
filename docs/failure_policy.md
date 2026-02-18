# Moon System Failure Policy

## Principles

1. Archive before any destructive reduction.
2. Prefer degraded operation over hard stop.
3. Always emit audit detail for failures and fallbacks.

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
