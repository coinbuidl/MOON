# 🌙 Moon System

A high-performance Rust CLI designed to orchestrate **OpenClaw** session contexts. It optimizes the context window by minimizing token usage while ensuring the agent retains seamless retrieval of historical knowledge.

## Core Features

1.  **Automated Lifecycle Watcher**: Monitors OpenClaw session and context size in real-time. Upon reaching defined thresholds, it triggers archiving, indexing, and compaction to prevent prompt overflow and minimize API costs.
2.  **Semantic Context Retrieval**: Provides the agent with a dedicated search interface to retrieve original, uncompacted context from archives whenever high-fidelity recall is required.
3.  **Tiered Distillation Pipeline**: 
    *   **Phase 1 (Raw Distillation)**: Automatically distills archived sessions into daily logs (`memory/YYYY-MM-DD.md`) using cost-effective model tiers.
    *   **Phase 2 (Strategic Integration)**: Facilitates the "upgrade" of daily insights into the global `MEMORY.md` by the primary agent.

## Recommended Agent Integration

To ensure reliable long-term memory and optimal token hygiene, it is recommended to explicitly define the boundary between the **Moon System** (automated) and the **Agent** (strategic) within your workspace rules (e.g., `AGENTS.md`):

*   **Moon System (Automated Lifecycle)**: Handles technical execution—token compaction, short-term session state maintenance, and daily raw context distillation (writes to `memory/YYYY-MM-DD.md`).
*   **Agent (Strategic Distillation)**: Responsible for high-level cognitive review—auditing daily logs and migrating key strategic insights into the long-term `MEMORY.md`.

This modular architecture prevents the Agent from being overwhelmed by raw session data while ensuring that distilled knowledge is persisted with high signal-to-noise ratios.

## Quick start

```bash
cp .env.example .env
cargo build
```

Run a few basics:

```bash
cargo run -- status
cargo run -- install --dry-run
cargo run -- install
cargo run -- moon-status
```

## CLI

Binary name: `oc-token-optim`

```bash
cargo run -- <command> [flags]
```

Global flag:

1. `--json` outputs machine-readable `CommandReport`

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

Exit codes:

1. `0` command completed with `ok=true`
2. `2` command completed with `ok=false`
3. `1` runtime/process error

## Common workflows

After OpenClaw upgrade:

```bash
cargo run -- post-upgrade
```

Archive and index latest session:

```bash
cargo run -- moon-snapshot
cargo run -- moon-index --name history
```

Recall prior context:

```bash
cargo run -- moon-recall --name history --query "your query"
```

Run one watcher cycle:

```bash
cargo run -- moon-watch --once
```

## Configuration

The CLI autoloads `.env` on startup (if present).

Start from:

1. `.env.example`
2. `moon.toml.example`

Most-used variables:

1. `OPENCLAW_BIN`
2. `QMD_BIN`
3. `MOON_HOME`
4. `OPENCLAW_SESSIONS_DIR`
5. `MOON_INBOUND_WATCH_PATHS`
6. `MOON_THRESHOLD_ARCHIVE_RATIO`
7. `MOON_THRESHOLD_PRUNE_RATIO`
8. `MOON_THRESHOLD_DISTILL_RATIO`
9. `MOON_POLL_INTERVAL_SECS`
10. `MOON_COOLDOWN_SECS`
11. `GEMINI_API_KEY` (for distillation)
12. `MOON_GEMINI_MODEL`

## Repository map

1. `src/cli.rs`: argument parsing + command dispatch
2. `src/commands/*.rs`: top-level command handlers
3. `src/openclaw/*.rs`: OpenClaw config/plugin/gateway operations
4. `src/moon/*.rs`: snapshot/index/recall/distill/watch logic
5. `assets/plugin/*`: plugin files embedded and installed by `install`
6. `tests/*.rs`: regression tests
7. `docs/*`: deeper operational docs

## Detailed docs

1. `docs/runbook.md`
2. `docs/contracts.md`
3. `docs/failure_policy.md`
4. `docs/security_checklist.md`

## Uninstall (quick)

If you need full cleanup, stop services and remove plugin/runtime data:

```bash
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.lilac.moon-system.plist 2>/dev/null || true
systemctl --user stop moon-system 2>/dev/null || true
systemctl --user disable moon-system 2>/dev/null || true

rm -f ~/Library/LaunchAgents/com.lilac.moon-system.plist
rm -f ~/.config/systemd/user/moon-system.service
systemctl --user daemon-reload 2>/dev/null || true

openclaw plugins uninstall oc-token-optim 2>/dev/null || true
rm -rf ~/.openclaw/extensions/oc-token-optim
rm -rf ~/.lilac_metaflora/archives ~/.lilac_metaflora/continuity ~/.lilac_metaflora/state ~/.lilac_metaflora/skills/moon-system/logs ~/.lilac_metaflora/memory
rm -f ~/.lilac_metaflora/MEMORY.md
```
