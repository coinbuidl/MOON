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
moon moon-watch --once
```

Distill trigger behavior:

1. Use `distill.mode = "idle"` with `distill.idle_secs = 360` for active OpenClaw environments.
2. Use `distill.mode = "daily"` for once-per-residential-day layer-2 distillation after `distill.idle_secs` (set `distill.residential_timezone`, e.g. `Australia/Sydney`).
3. `distill.mode = "manual"` disables automatic layer-2 distillation.
4. Auto-distill reads archive projection markdown (`archives/mlib/*.md`) as its source.
5. Selection order is oldest pending archive day first, then up to `max_per_cycle`.
6. Start with `max_per_cycle=1` in test stage, then increase after stable runs.
7. When `distill.topic_discovery = true`, daily memory files maintain a top `Entity Anchors` block with discovered topic tags.

Retention windows:

1. Active (`<=7` days), warm (`8-30` days), cold candidate (`>=31` days).
2. Cold deletion requires a distill marker in state for that archive.

## Start Daemon

```bash
moon moon-watch --daemon
```

## Manual Distill

```bash
moon moon-distill --archive $MOON_ARCHIVES_DIR/raw/<file>.jsonl --session-id <id>
```

Manual layer-2 queue trigger (same selection logic as watcher):

```bash
moon moon-watch --once --distill-now
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

## Troubleshooting

1. No usage data:
- verify `openclaw` is available on `PATH` (`command -v openclaw`)
- optionally set `OPENCLAW_BIN` to a specific `openclaw` binary path
2. QMD indexing/search fails:
- set `QMD_BIN`
- verify `qmd collection add` and `qmd search` work manually
3. Distill not using Gemini:
- set `GEMINI_API_KEY`
- optional model override: `MOON_GEMINI_MODEL`
4. Session rollover fails:
- set `MOON_SESSION_ROLLOVER_CMD` to your environment-specific command
- continuity map still persists with `rollover_ok=false`
