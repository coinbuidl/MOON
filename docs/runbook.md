# M.O.O.N. Runbook

## Start One Cycle

```bash
moon moon-watch --once
```

Bootstrap sequence (minimal setup):

```bash
cp .env.example .env
cp moon.toml.example moon.toml
moon verify --strict
moon moon-status
moon moon-health
moon config --show
moon moon-watch --once
```

Distill trigger behavior:

1. Watcher L1 normalisation is always auto-check, gated by `watcher.cooldown_secs`.
2. Auto L1 source is projection markdown only: `archives/mlib/*.md` (never raw JSONL).
3. Auto L1 selects pending projections deterministically and applies `distill.max_per_cycle`.
4. Auto L1 uses a non-blocking lock; lock contention degrades/skips this cycle.
5. Auto `syns` runs once per residential day on the first watcher cycle after local midnight.
6. Auto `syns` blends yesterday's daily memory file (`memory/YYYY-MM-DD.md`) with current `memory.md` (when present), then rewrites `memory.md`.
7. Start with `max_per_cycle=1` in test stage, then increase after stable runs.
8. When `distill.topic_discovery = true`, daily memory files maintain a top `Entity Anchors` block with discovered topic tags.

Retention windows:

1. Active (`<= active_days`), warm (`<= warm_days`), cold candidate (`>= cold_days`).
2. Cold deletion requires a distill marker in state for that archive.

## Start Daemon

```bash
moon moon-watch --daemon
```

## Health Probe

```bash
moon moon-health
```

## Manual Distill

```bash
moon distill -mode norm -archive $MOON_ARCHIVES_DIR/mlib/<file>.md -session-id <id>
```

Rules:
1. `-archive` is required and must point to a readable `archives/mlib/*.md` file.
2. The file must be pending (indexed and not yet normalised in state ledger).
3. Manual norm is immediate and bypasses watcher cooldown, but still requires L1 lock availability.

Manual Layer-2 distillation:

```bash
moon distill -mode syns
```

Recommended `syns` model config (high reasoning quality):

```bash
MOON_WISDOM_PROVIDER=openai
MOON_WISDOM_MODEL=gpt-4.1
OPENAI_API_KEY=...
```

Default `syns` sources are today's `memory/YYYY-MM-DD.md` plus current `memory.md`.

Layer-2 distillation from explicit file set only:

```bash
moon distill -mode syns -file $MOON_MEMORY_DIR/2026-03-01.md -file $MOON_MEMORY_DIR/2026-03-02.md
```

When `-file` is provided, only the listed files participate. `memory.md` is included only if explicitly listed.

Manual L1 queue trigger (same selection logic as watcher):

```bash
moon moon-watch --once --distill-now
```

Dry-run watcher cycle (no state/archive mutations):

```bash
moon moon-watch --once --dry-run
```

## Recall

```bash
moon moon-recall --query "keyword" --name history
```

Rebuild history index + normalize archive layout:

```bash
moon moon-index --name history
```

## Key Paths

1. State file: `$MOON_STATE_FILE` (default: `$MOON_HOME/moon/state/moon_state.json`; `MOON_STATE_DIR` is supported as directory override)
2. Archives root: `$MOON_ARCHIVES_DIR` (default: `$MOON_HOME/archives`)
3. Raw session snapshots: `$MOON_ARCHIVES_DIR/raw/*.jsonl`
4. Archive projections for retrieval: `$MOON_ARCHIVES_DIR/mlib/*.md`
5. Archive ledger: `$MOON_ARCHIVES_DIR/ledger.jsonl`
6. Daily memory: `$MOON_MEMORY_DIR/YYYY-MM-DD.md` (default: `$MOON_HOME/memory/YYYY-MM-DD.md`)
7. Audit log: `$MOON_LOGS_DIR/audit.log` (default: `$MOON_HOME/moon/logs/audit.log`)
8. Daemon lock: `$MOON_LOGS_DIR/moon-watch.daemon.lock` (JSON payload includes `pid`, `started_at_epoch_secs`, `build_uuid`, `moon_home`)

## Troubleshooting

1. No usage data:
- verify `openclaw` is available on `PATH` (`command -v openclaw`)
- optionally set `OPENCLAW_BIN` to a specific `openclaw` binary path
2. QMD indexing/search fails:
- set `QMD_BIN`
- verify `qmd collection add` and `qmd search` work manually
3. `syns` not using remote reasoning model:
- set one provider API key (`OPENAI_API_KEY` or `ANTHROPIC_API_KEY` or `GEMINI_API_KEY` or `AI_API_KEY`)
- set `MOON_WISDOM_PROVIDER` and `MOON_WISDOM_MODEL`
4. Session rollover fails:
- set `MOON_SESSION_ROLLOVER_CMD` to your environment-specific command
- continuity map still persists with `rollover_ok=false`
5. Mutating command fails with out-of-bounds error:
- run from your workspace tree, or use global escape hatch `--allow-out-of-bounds`
