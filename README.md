# moon-system

Rust CLI for installing, verifying, repairing, and indexing the `oc-token-optim` OpenClaw plugin plus Moon snapshot workflows.

## Purpose

This repo provides an idempotent operations tool for two domains:

1. OpenClaw plugin lifecycle (`install`, `status`, `verify`, `repair`, `post-upgrade`)
2. Moon archive/index workflows (`moon-status`, `moon-snapshot`, `moon-index`)

## CLI Surface

Binary name: `oc-token-optim`

```bash
cargo run -- <command> [flags]
```

Commands:

1. `install [--force] [--dry-run] [--apply true|false]`
2. `status`
3. `verify [--strict]`
4. `repair [--force]`
5. `post-upgrade`
6. `moon-status`
7. `moon-snapshot [--source <path>] [--dry-run]`
8. `moon-index [--name <collection>] [--dry-run]`
9. `moon-watch [--once|--daemon]`
10. `moon-recall --query <text> [--name <collection>]`
11. `moon-distill --archive <path> [--session-id <id>]`

Global flags:

1. `--json` for machine-readable `CommandReport`

Exit behavior:

1. `0` when report `ok=true`
2. `2` when report `ok=false`
3. `1` on runtime error

## Command Functions

### OpenClaw workflow

1. `install`
   - Syncs embedded plugin assets from `assets/plugin/*` to `~/.openclaw/extensions/oc-token-optim`
   - Patches config defaults in `openclaw.json` (JSON/JSON5 supported)
   - Forces `plugins.entries.oc-token-optim.enabled=true`
   - Writes config atomically and creates timestamped backup when file exists

2. `status`
   - Resolves OpenClaw paths
   - Verifies plugin presence and content parity with local assets
   - Checks plugin listing/load state from `openclaw plugins list --json`
   - Validates required config keys and reports gaps as issues

3. `verify`
   - Runs `status`
   - Requires OpenClaw binary availability
   - Runs `openclaw doctor` (non-interactive first, fallback interactive)

4. `repair`
   - Force-installs plugin/config
   - Restarts gateway (`openclaw gateway restart` with stop/start fallback)
   - Runs strict verify

5. `post-upgrade`
   - Runs install + gateway restart + strict verify
   - If verify fails, auto-runs `repair` fallback

### Moon workflow

1. `moon-status`
   - Resolves Moon paths and reports required dirs/files/binaries

2. `moon-snapshot`
   - Source file is latest session in `~/.openclaw/agents/main/sessions` unless `--source` is provided
   - Writes raw copy into archives dir with `{slug}-{epoch}.{ext}` naming

3. `moon-index`
   - Runs `qmd collection add <archives_dir> --name <collection>`

4. `moon-watch`
   - Runs continuous or one-shot watcher cycles
   - Collects usage (OpenClaw primary, session-file fallback)
   - Evaluates archive/prune/distill thresholds
   - Executes pipeline: archive -> qmd index -> prune profile -> distill -> continuity map

5. `moon-recall`
   - Runs `qmd search <collection> <query> --json`
   - Returns ranked matches for context rehydration

6. `moon-distill`
   - Manual distillation trigger from an archive file
   - Writes distilled output to daily memory log

## Config Defaults Applied by `install`

Core defaults under `agents.defaults`:

1. `compaction.reserveTokensFloor = 24000`
2. `compaction.maxHistoryShare = 0.6`
3. `contextPruning.mode = "cache-ttl"`
4. `contextPruning.softTrim.maxChars = 4000`
5. `contextPruning.softTrim.headChars = 1500`
6. `contextPruning.softTrim.tailChars = 1500`

Channel defaults (for each configured channel provider):

1. `historyLimit = 50`
2. `dmHistoryLimit = 30`

Plugin defaults under `plugins.entries.oc-token-optim.config`:

1. `maxTokens = 12000`
2. `maxChars = 60000`
3. `maxRetainedBytes = 250000`
4. Tool-specific token/char limits for:
   - `read`
   - `message/readMessages`
   - `message/searchMessages`
   - `web_fetch`
   - `web.fetch`

## Environment Variables

The CLI auto-loads `.env` from the repo root on startup (if present).

Recommended setup:

1. Copy `.env.example` to `.env`
2. Set your own `GEMINI_API_KEY`
3. Optional overrides:
   - `OPENCLAW_BIN`
   - `QMD_BIN`
   - `MOON_HOME`
   - `OPENCLAW_SESSIONS_DIR`
   - `GEMINI_API_KEY`
4. Keep `.env` uncommitted (`.gitignore` already excludes it)

OpenClaw path overrides:

1. `OPENCLAW_BIN`
2. `OPENCLAW_HOME`
3. `OPENCLAW_STATE_DIR`
4. `OPENCLAW_CONFIG_PATH`

Moon path overrides:

1. `MOON_HOME`
2. `MOON_ARCHIVES_DIR`
3. `MOON_MEMORY_DIR`
4. `MOON_MEMORY_FILE`
5. `MOON_LOGS_DIR`
6. `OPENCLAW_SESSIONS_DIR`
7. `QMD_BIN`
8. `QMD_DB`
9. `MOON_CONFIG_PATH`
10. `MOON_THRESHOLD_ARCHIVE_RATIO`
11. `MOON_THRESHOLD_PRUNE_RATIO`
12. `MOON_THRESHOLD_DISTILL_RATIO`
13. `MOON_POLL_INTERVAL_SECS`
14. `MOON_COOLDOWN_SECS`
15. `MOON_ENABLE_PRUNE_WRITE`
16. `MOON_ENABLE_SESSION_ROLLOVER`
17. `MOON_SESSION_ROLLOVER_CMD`
18. `MOON_OPENCLAW_USAGE_ARGS`
19. `GEMINI_API_KEY`
20. `MOON_GEMINI_MODEL`

## Repo Map

1. `src/cli.rs`: clap parsing, dispatch, report printing
2. `src/commands/*.rs`: command handlers
3. `src/openclaw/*.rs`: OpenClaw config/path/gateway/plugin logic
4. `src/moon/*.rs`: Moon paths, snapshot writer, QMD integration
5. `src/assets.rs`: embedded plugin asset loading/writing
6. `assets/plugin/*`: plugin files copied into OpenClaw extensions
7. `tests/*.rs`: command and flow regressions
8. `docs/*`: contracts, failure policy, runbook, security checklist
9. `deploy/*`: launchd and systemd service templates

## Agent Usage Notes

1. Prefer `--json` for automation and parse `ok`, `details`, `issues`.
2. Use `install --dry-run` before mutating config in safety-critical runs.
3. Use `post-upgrade` as the default recovery entrypoint after OpenClaw upgrades.
4. Use `moon-snapshot` then `moon-index` for archive ingestion workflows.

## Use As Skill

You can use this repo from your agent skill folder.

Recommended setup:
1. Keep this codebase in a normal git repo path:
   - `/Users/lilac/gh/moon-system`
2. Create a lightweight skill wrapper folder:
   - `$CODEX_HOME/skills/moon-system/`
3. Add `SKILL.md` in that wrapper folder that points to this repo and defines when to run:
   - `cargo run -- moon-watch --once`
   - `cargo run -- moon-recall --query \"...\" --name history`
   - `cargo run -- moon-distill --archive <path>`
4. Set required environment variables in the runtime environment:
   - `OPENCLAW_BIN`, `QMD_BIN`, `MOON_HOME`
   - `GEMINI_API_KEY` (optional, for Gemini distillation)

Alternative:
1. You can copy this whole repo into a skill folder, but wrapper-style is easier to keep updated.

## Complete Uninstall Guide

Run this if you want to remove Moon System + `oc-token-optim` completely.

1. Stop background watcher services (if installed):

```bash
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.lilac.moon-system.plist 2>/dev/null || true
systemctl --user stop moon-system 2>/dev/null || true
systemctl --user disable moon-system 2>/dev/null || true
```

2. Remove service definitions:

```bash
rm -f ~/Library/LaunchAgents/com.lilac.moon-system.plist
rm -f ~/.config/systemd/user/moon-system.service
systemctl --user daemon-reload 2>/dev/null || true
```

3. Remove plugin install from OpenClaw:

```bash
openclaw plugins uninstall oc-token-optim 2>/dev/null || true
rm -rf ~/.openclaw/extensions/oc-token-optim
```

4. Remove Moon runtime data (archives, logs, continuity, state, daily memory):

```bash
rm -rf ~/.lilac_metaflora/archives
rm -rf ~/.lilac_metaflora/continuity
rm -rf ~/.lilac_metaflora/state
rm -rf ~/.lilac_metaflora/skills/moon-system/logs
rm -rf ~/.lilac_metaflora/memory
```

5. Optional: remove long-term memory file created for Moon workflows:

```bash
rm -f ~/.lilac_metaflora/MEMORY.md
```

6. Remove local secret/env configuration:

```bash
rm -f .env
```

7. Optional: clean local Rust build artifacts for this repo:

```bash
cargo clean
```

8. Verify uninstall:

```bash
openclaw plugins list --json | rg oc-token-optim || true
test ! -d ~/.openclaw/extensions/oc-token-optim && echo "plugin removed"
test ! -d ~/.lilac_metaflora/archives && echo "moon archives removed"
```

Note:
1. If you want to keep historical memory data, skip step 4 and step 5.
2. If you want to keep other `.lilac_metaflora` assets, delete only the listed subpaths.

## Development

```bash
cargo fmt --all
cargo check
cargo clippy -- -D warnings
cargo test
```
