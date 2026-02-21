# 🌙 M.O.O.N.
> **Strategic Memory Augmentation & Context Distillation System**

```text
[SYSTEM BOOT... PHASE 1: NEURAL LINK ESTABLISHED]
[LOADING EXTERNAL MEMORY MODULE: M.O.O.N.]
```

### <span style="font-family:'Orbitron','Bank Gothic','Eurostile',sans-serif;"><font color="#dd0000">M</font>emory</span>
### <span style="font-family:'Orbitron','Bank Gothic','Eurostile',sans-serif;"><font color="#dd0000">O</font>ptimisation</span>
### <span style="font-family:'Orbitron','Bank Gothic','Eurostile',sans-serif;"><font color="#dd0000">O</font>perational</span>
### <span style="font-family:'Orbitron','Bank Gothic','Eurostile',sans-serif;"><font color="#dd0000">N</font>ormaliser</span>

---

## 🛰️ Tactical Overview
**M.O.O.N.** is a high-performance, background-active memory optimiser designed to enhance AI systems with autonomous memory management. Like a tactical drone deployed in the heat of battle, it monitors, archives, and distills overwhelming context streams into high-signal structural intelligence.

It optimizes the **OpenClaw** context window by minimizing token usage while ensuring the agent retains seamless retrieval of historical knowledge.

## Core Features

1.  **Automated Lifecycle Watcher**: Monitors OpenClaw session and context size in real-time. Upon reaching defined thresholds, it triggers archiving, indexing, and compaction to prevent prompt overflow and minimize API costs.
    * During compaction, Moon writes a deterministic `[MOON_ARCHIVE_INDEX]` note into the active session so agents can locate pre-compaction archives.
2.  **Semantic Context Retrieval**: Moon writes a structured v2 markdown projection (`archives/raw/*.md`) for each raw session archive (`archives/raw/*.jsonl`). Projections include:
    * Timeline table with UTC + local timestamps
    * Conversation summaries (user queries / assistant responses)
    * Tool activity with contextual stitching (toolUse → toolResult coupling)
    * Keywords, topics, and compaction anchors
    * Natural language time markers for improved semantic recall
    * Side-effect priority classification for tool entries
3.  **Tiered Distillation Pipeline**:
    *   **Phase 1 (Raw Distillation)**: Automatically distills archive projection markdown (`archives/raw/*.md`) into daily logs (`memory/YYYY-MM-DD.md`) using cost-effective model tiers.
    *   **Phase 2 (Strategic Integration)**: Facilitates the "upgrade" of daily insights into the global `MEMORY.md` by the primary agent.

## Recommended Agent Integration

To ensure reliable long-term memory and optimal token hygiene, it is recommended to explicitly define the boundary between the **M.O.O.N.** (automated) and the **Agent** (strategic) within your workspace rules (e.g., `AGENTS.md`):

*   **M.O.O.N. (Automated Lifecycle)**: Handles technical execution—token compaction, short-term session state maintenance, and daily raw context distillation (writes to `memory/YYYY-MM-DD.md`).
*   **Agent (Strategic Distillation)**: Responsible for high-level cognitive review—auditing daily logs and migrating key strategic insights into the long-term `MEMORY.md`.

This modular architecture prevents the Agent from being overwhelmed by raw session data while ensuring that distilled knowledge is persisted with high signal-to-noise ratios.

### AGENTS.md Recall Policy Template

Add this block to your workspace `AGENTS.md` (adjust the repo path if different):

```md
### MOON Archive Recall Policy (Required)

1. History search backend is QMD collection `history`, rooted at `~/.lilac_metaflora/archives`, mask `**/*.md` (archive projections in `~/.lilac_metaflora/archives/raw/*.md`).
2. Default history retrieval command is `cargo run --manifest-path /Users/lilac/.lilac_metaflora/skills/moon-system/Cargo.toml -- moon-recall --name history --query "<user-intent-query>"`.
3. Run history retrieval before answering when any condition is true: user references past sessions, pre-compaction context, prior decisions, or current-session context is insufficient.
4. Retrieval procedure is strict: run one primary query, run one fallback query if no hits, and use top 3 hits only; include `archive_path` in reasoning when available.
5. If finer detail is required, read the projection frontmatter field `archive_jsonl_path` and fetch only the minimal raw JSONL segment needed.
6. If both primary and fallback queries return no relevant hit, explicitly reply `HISTORY_NOT_FOUND` (cannot find in archives).
7. Never fabricate prior-session facts when `moon-recall` returns no relevant match.
```

Query semantics:

1. Primary query: direct user intent in natural language.
2. Fallback query: broader keywords from the same intent when primary has no relevant match.
3. Top 3 hits: highest-score results returned by `moon-recall`.

## Agent bootstrap checklist

1. Set `.env` (at minimum: `OPENCLAW_BIN`; recommended: explicit path block below).
2. Validate environment and plugin wiring:
   `cargo run -- verify --strict`
3. Check Moon runtime paths:
   `cargo run -- moon-status`
4. Run one watcher cycle:
   `cargo run -- moon-watch --once`
5. Enable daemon mode only after one-shot run is clean.

## Quick start

```bash
cp .env.example .env
cargo build
```

Set `.env` before first run.

Must-have variable:

```bash
# Required: OpenClaw binary path (no default)
OPENCLAW_BIN=/absolute/path/to/openclaw
```

Recommended explicit path setup (these are the runtime defaults, written explicitly for clarity):

```bash
# Binaries
# QMD is an external dependency (separate repo/project). Moon only calls its CLI.
QMD_BIN=$HOME/.bun/bin/qmd
QMD_DB=$HOME/.cache/qmd/index.sqlite

# Moon runtime paths
MOON_HOME=$HOME/.lilac_metaflora
MOON_ARCHIVES_DIR=$MOON_HOME/archives
MOON_MEMORY_DIR=$MOON_HOME/memory
MOON_MEMORY_FILE=$MOON_HOME/MEMORY.md
MOON_LOGS_DIR=$MOON_HOME/skills/moon-system/logs
MOON_CONFIG_PATH=$MOON_HOME/moon.toml

# OpenClaw session source
OPENCLAW_STATE_DIR=$HOME/.openclaw
OPENCLAW_CONFIG_PATH=$OPENCLAW_STATE_DIR/openclaw.json
OPENCLAW_SESSIONS_DIR=$HOME/.openclaw/agents/main/sessions
```

Cheaper distill profile (recommended for the agent):

```bash
# Distillation is the only stage that needs an LLM API key.
# Use a low-cost model for daily distill jobs.
MOON_DISTILL_PROVIDER=gemini
MOON_DISTILL_MODEL=gemini-2.5-flash-lite
GEMINI_API_KEY=...
```

Distill safety guardrails (recommended):

```bash
# Distill trigger behavior:
# - mode=idle: run distill after no new archives for idle_secs.
# - recommended idle window for active OpenClaw usage: 360 seconds (6 minutes).
MOON_DISTILL_MODE=idle
MOON_DISTILL_IDLE_SECS=360
# During validation start with 1 and raise after stable runs.
MOON_DISTILL_MAX_PER_CYCLE=1

# Retention windows (days)
MOON_RETENTION_ACTIVE_DAYS=7
MOON_RETENTION_WARM_DAYS=30
MOON_RETENTION_COLD_DAYS=31

# Archives larger than this threshold are chunk-distilled automatically.
# Use `auto` to infer a safe chunk size from model context limits
# when the provider exposes them (fallback heuristics are applied).
# `auto` is also the runtime default if this variable is unset.
MOON_DISTILL_CHUNK_BYTES=auto

# Safety ceiling for number of chunks processed per archive run.
MOON_DISTILL_MAX_CHUNKS=128

# Optional explicit model context hint for `auto` mode.
# MOON_DISTILL_MODEL_CONTEXT_TOKENS=250000

# Background watcher alert threshold for extreme token usage (0 disables alert).
# Default is 1,000,000 tokens.
MOON_HIGH_TOKEN_ALERT_THRESHOLD=1000000
```

Cheapest possible mode (zero API cost, local-only distillation):

```bash
MOON_DISTILL_PROVIDER=local
```

Run a few basics:

```bash
cargo run -- status
cargo run -- install --dry-run
cargo run -- install
cargo run -- moon-status
```

## CLI

Binary name: `MOON`

Note: OpenClaw plugin id remains `oc-token-optim` for compatibility with
existing installs, but the runtime CLI process name is now `MOON`.

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
8. `moon-index [--name <collection>] [--dry-run] [--reproject]`
   - `--reproject`: regenerate all projection markdown files using the v2 structured format
9. `moon-watch [--once|--daemon]`
10. `moon-recall --query <text> [--name <collection>]`
11. `moon-distill --archive <path> [--session-id <id>] [--allow-large-archive]`
    - default: archives larger than `MOON_DISTILL_CHUNK_BYTES` are auto-distilled in chunks
    - `--allow-large-archive`: force single-pass distill above the chunk threshold

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

`moon-index` also normalizes older archive layout into `archives/raw/` and backfills missing projection markdown files before running QMD sync.

Recall prior context:

```bash
cargo run -- moon-recall --name history --query "your query"
```

Run one watcher cycle:

```bash
cargo run -- moon-watch --once
```

Idle distill selection order:

1. Distill waits until the latest archive has been idle for `MOON_DISTILL_IDLE_SECS`.
2. It then selects the oldest pending archive day first.
3. It distills projection markdown sidecars (`*.md`) for those archives, not raw `*.jsonl`.
4. It processes up to `max_per_cycle` archives from that day.

Retention lifecycle windows:

1. Active (`<= active_days`): keep archives for fast debug/resume.
2. Warm (`active_days < age <= warm_days`): retained and indexed.
3. Cold candidate (`>= cold_days`): deleted only when a distill marker exists.

Archive layout:

1. `archives/ledger.jsonl`: archive ledger metadata.
2. `archives/raw/*.jsonl`: raw snapshot copy (full fidelity).
3. `archives/raw/*.md`: noise-reduced projection indexed by QMD.

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
5. `MOON_DISTILL_PROVIDER`
6. `MOON_DISTILL_MODEL`
7. `GEMINI_API_KEY` / `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `AI_API_KEY` (distill only)
8. `MOON_DISTILL_CHUNK_BYTES` (default `auto`; use numeric bytes to force a fixed threshold)
9. `MOON_DISTILL_MAX_CHUNKS` (default `128`)
10. `MOON_DISTILL_MODEL_CONTEXT_TOKENS` (optional context hint used by `MOON_DISTILL_CHUNK_BYTES=auto`)
11. `MOON_HIGH_TOKEN_ALERT_THRESHOLD` (default `1000000`; set `0` to disable)
12. `MOON_THRESHOLD_ARCHIVE_RATIO`
13. `MOON_THRESHOLD_COMPACTION_RATIO`
14. `MOON_POLL_INTERVAL_SECS`
15. `MOON_COOLDOWN_SECS`
16. `MOON_INBOUND_WATCH_PATHS`
17. `MOON_RETENTION_ACTIVE_DAYS` / `MOON_RETENTION_WARM_DAYS` / `MOON_RETENTION_COLD_DAYS`

## Repository map

1. `src/cli.rs`: argument parsing + command dispatch
2. `src/commands/*.rs`: top-level command handlers
3. `src/openclaw/*.rs`: OpenClaw config/plugin/gateway operations
4. `src/moon/*.rs`: snapshot/index/recall/distill/watch logic
   - `src/moon/util.rs`: shared utilities (`now_epoch_secs`, `truncate_with_ellipsis`)
5. `assets/plugin/*`: plugin files embedded and installed by `install`
6. `tests/*.rs`: regression tests
7. `docs/*`: deeper operational docs
8. `audit_report.md`: latest code audit findings and fixes

## Detailed docs

1. `docs/runbook.md`
2. `docs/contracts.md`
3. `docs/failure_policy.md`
4. `docs/security_checklist.md`

## Uninstall (quick)

If you need full cleanup, stop services and remove plugin/runtime data:

```bash
# Stop known service names (current + legacy)
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.lilac.moon-system.plist 2>/dev/null || true
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.lilac.moon.plist 2>/dev/null || true
systemctl --user stop moon-system 2>/dev/null || true
systemctl --user disable moon-system 2>/dev/null || true
systemctl --user stop moon 2>/dev/null || true
systemctl --user disable moon 2>/dev/null || true

rm -f ~/Library/LaunchAgents/com.lilac.moon-system.plist
rm -f ~/Library/LaunchAgents/com.lilac.moon.plist
rm -f ~/.config/systemd/user/moon-system.service
rm -f ~/.config/systemd/user/moon.service
systemctl --user daemon-reload 2>/dev/null || true

openclaw plugins uninstall oc-token-optim 2>/dev/null || true
rm -rf ~/.openclaw/extensions/oc-token-optim

MOON_HOME="${MOON_HOME:-$HOME/.lilac_metaflora}"
rm -rf "$MOON_HOME/archives" "$MOON_HOME/continuity" "$MOON_HOME/state" "$MOON_HOME/memory"
rm -rf "$MOON_HOME/skills/moon-system/logs"
rm -f "$MOON_HOME/MEMORY.md"

# Optional: remove persisted Moon config if you created one
rm -f "$MOON_HOME/moon.toml"
```

Note: uninstalling the plugin does not automatically restore custom OpenClaw
config values previously written under `plugins.entries.oc-token-optim` or
`agents.defaults.*`. Remove or revert those keys manually in
`~/.openclaw/openclaw.json` if you want a full config rollback.
