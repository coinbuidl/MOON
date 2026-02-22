# Moon System Runbook

## Start One Cycle

```bash
cargo run -- moon-watch --once
```

Bootstrap sequence (minimal setup):

```bash
cp .env.example .env
cargo run -- verify --strict
cargo run -- moon-status
cargo run -- moon-watch --once
```

Distill trigger behavior:

1. Use `distill.mode = "idle"` with `distill.idle_secs = 360` for active OpenClaw environments.
2. Use `distill.mode = "daily"` for once-per-residential-day layer-2 distillation after `distill.idle_secs` (set `distill.residential_timezone`, e.g. `Australia/Sydney`).
3. `distill.mode = "manual"` disables automatic layer-2 distillation.
4. Auto-distill reads archive projection markdown (`archives/mlib/*.md`) as its source.
5. Selection order is oldest pending archive day first, then up to `max_per_cycle`.
6. Start with `max_per_cycle=1` in test stage, then increase after stable runs.
7. When `MOON_TOPIC_DISCOVERY=true`, daily memory files maintain a top `Entity Anchors` block with discovered topic tags.

Retention windows:

1. Active (`<=7` days), warm (`8-30` days), cold candidate (`>=31` days).
2. Cold deletion requires a distill marker in state for that archive.

## Start Daemon

```bash
cargo run -- moon-watch --daemon
```

## Manual Distill

```bash
cargo run -- moon-distill --archive ~/.lilac_metaflora/archives/raw/<file>.jsonl --session-id <id>
```

Manual layer-2 queue trigger (same selection logic as watcher):

```bash
cargo run -- moon-watch --once --distill-now
```

## Recall

```bash
cargo run -- moon-recall --query "keyword" --name history
```

Rebuild history index + normalize archive layout:

```bash
cargo run -- moon-index --name history
```

## Key Paths

1. State file: `~/.lilac_metaflora/state/moon_state.json`
2. Archives: `~/.lilac_metaflora/archives/`
3. Raw session snapshots: `~/.lilac_metaflora/archives/raw/*.jsonl`
4. Archive projections for retrieval: `~/.lilac_metaflora/archives/mlib/*.md`
5. Archive ledger: `~/.lilac_metaflora/archives/ledger.jsonl`
6. Daily memory: `~/.lilac_metaflora/memory/YYYY-MM-DD.md`
7. Audit log: `~/.lilac_metaflora/skills/moon-system/logs/audit.log`

## Troubleshooting

1. No usage data:
- verify `OPENCLAW_BIN` is set to a valid `openclaw` binary path
2. QMD indexing/search fails:
- set `QMD_BIN`
- verify `qmd collection add` and `qmd search` work manually
3. Distill not using Gemini:
- set `GEMINI_API_KEY`
- optional model override: `MOON_GEMINI_MODEL`
4. Session rollover fails:
- set `MOON_SESSION_ROLLOVER_CMD` to your environment-specific command
- continuity map still persists with `rollover_ok=false`
