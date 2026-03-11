# MOON v1 Simplification Plan

Date: 2026-03-11
Branch: `codex/moon-v1`
Scope: simplify the codebase to one clean primary flow, with fallback deferred until the final phase

## Rule

This rule is now strict:

1. Keep the primary flow simple and direct.
2. Do not carry legacy parallel paths.
3. Do not keep fallback behavior in the main path.
4. Delete dead code, duplicate paths, and transitional compatibility code as soon as the new path is established.
5. Add fallback only in the final phase.

## Current Intended Product Direction

The current desired direction is narrower than the earlier v1 plan.

Primary runtime responsibilities now are:

1. `watch` processes `$MOON_HOME/mds/*.md` into `$HOME/memory/YYYY-MM-DD.md`.
2. `watch` runs daily `syns` at midnight.

Primary data locations now are:

1. `$MOON_HOME/raw/*`
2. `$MOON_HOME/mds/*.md`

Things that must not exist in the primary flow:

1. `archives/raw/*`
2. `archives/mlib/*.md`
3. `archives/cleanse/*`
4. OpenClaw-managed continuation/index note behavior in the normal path
5. mixed watcher responsibilities like compaction, archive retention, channel archive maps, or usage-trigger logic

## Today's Milestones

Completed today:

1. Re-scoped `watch` to the simplified runtime role:
   - pending `mds/*.md` -> L1 daily memory
   - midnight `syns`
2. Removed watcher-owned compaction/archive/continuation/index-note/embed/retention behavior from the compiled runtime.
3. Switched `distill norm` to accept `$MOON_HOME/mds/*.md` directly instead of old `mlib` projection discovery.
4. Switched embed document discovery from `archives/mlib` to `$MOON_HOME/mds`.
5. Removed legacy watcher-only modules from the compiled tree:
   - `context_engine`
   - `continuity`
   - `inbound_watch`
   - `session_usage`
   - `thresholds`
6. Removed legacy archive/projection command surface:
   - `snapshot`
   - `cleanse`
   - `index`
   - `recall`
7. Removed the legacy archive/projection modules:
   - `archive`
   - `channel_archive_map`
   - `cleanse`
   - `recall`
   - `snapshot`
8. Removed OpenClaw live-session continuation/index-note helpers from the normal runtime path.
9. Removed stale `mlib`-based and compaction-based tests and replaced watcher coverage with workflow-matching tests only.
10. Added a narrow watcher time test hook:
    - `MOON_WATCH_FAKE_NOW_EPOCH_SECS`

## Current Workflow

This section describes the workflow that is currently implemented in code after today’s purge.

### 1. Watch cycle

`moon watch --once` now does this:

1. Resolve runtime paths from `MOON_HOME`, `MOON_MEMORY_DIR`, `MOON_MEMORY_FILE`, and logs/state locations.
2. Load config.
3. Load state.
4. Scan `$MOON_HOME/mds` recursively for `*.md`.
5. Compare doc mtimes against `state.distilled_archives`.
6. Select pending `mds` docs, oldest first.
7. Run up to `distill.max_per_cycle` L1 normalisation jobs.
8. Record the distilled `mds` path back into state.
9. Check whether the current local hour is midnight in the configured residential timezone.
10. If midnight has not yet been processed for that local day, run `syns`.
11. Save state.

### 2. L1 normalisation path

For each pending `$MOON_HOME/mds/*.md` document:

1. `watch` passes the file path into `run_distillation(...)`.
2. L1 writes day-based output into `$HOME/memory/YYYY-MM-DD.md`.
3. The file path of the source `mds` doc is recorded in `state.distilled_archives`.

Current source-of-truth for L1:

1. `$MOON_HOME/mds/*.md`

### 3. Midnight syns path

At midnight local time:

1. `watch` checks `state.last_syns_trigger_epoch_secs`.
2. If `syns` has not already run for that local calendar day, it selects:
   - yesterday’s daily memory file, if present
   - `MEMORY.md`
3. It runs `run_wisdom_distillation(...)`.
4. It updates `state.last_syns_trigger_epoch_secs`.

### 4. Daemon mode

`moon watch --daemon` now only loops the simplified watch cycle above.

It no longer owns:

1. usage-triggered compaction
2. archive capture
3. continuation note injection
4. archive retention cleanup
5. channel archive continuity

## Current State Of The Codebase

Cleaned:

1. The active watcher path is substantially simpler.
2. The active L1 input contract is now `mds`.
3. The old `mlib`-centric command surface is removed.
4. The old OpenClaw note path is removed from the primary runtime.

Still not fully aligned with the desired end state:

1. `$MOON_HOME/raw/*` capture is not currently implemented in the simplified runtime.
2. The canonical `cleanse` contract has not been redefined in a single clean form.
3. `embed` still exists as a command/module and still depends on QMD, even though it is no longer owned by `watch`.
4. Some OpenClaw support code still exists for install/verify/repair/status/restart flows.
5. Docs and examples still likely describe older behavior in places.

## Left Over

High-priority remaining work:

1. Define the new `raw -> cleanse -> mds` primary pipeline explicitly.
2. Re-introduce raw capture only in the simplified path:
   - write active context snapshots to `$MOON_HOME/raw/*`
3. Define `cleanse` as one clean contract instead of multiple intermediate documents.
4. Decide the role of `embed`:
   - either remove it for now
   - or keep it as a separate explicit command outside `watch`
5. Update docs, examples, and operator guidance to match the narrowed runtime.

Likely purge candidates next:

1. `embed` command/module if it is considered outside the simplified primary runtime
2. dead QMD helpers left after command removal
3. stale README and bootstrap instructions that still mention archive/projection/compaction ownership

## Recommended Next Steps

Recommended next dev session:

1. Define the new raw capture contract:
   - source of active context
   - file naming
   - retention expectation
2. Replace the old broad cleanse idea with a single simple cleanse output contract.
3. Write `mds` generation from `raw` using that one contract.
4. Decide whether `embed` stays as an explicit manual phase or is removed until later.
5. Update README and runtime docs only after the code contract is stable.

## What Not To Do Next

Do not do these yet:

1. add fallback behavior
2. restore OpenClaw `/compact`
3. reintroduce `mlib`
4. keep dual storage layouts alive
5. re-add compatibility glue for deleted legacy paths

## Acceptance For The Next Phase

The next phase should be considered successful when:

1. `$MOON_HOME/raw/*` exists as the only raw capture location
2. `$MOON_HOME/mds/*.md` exists as the only rich markdown location
3. `cleanse` has one clear output contract
4. `watch` remains limited to:
   - `mds -> daily memory`
   - midnight `syns`
5. no legacy archive/projection path is reintroduced
