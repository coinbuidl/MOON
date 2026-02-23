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
2.  **Semantic Context Retrieval**: Moon writes a structured v2 markdown projection (`archives/mlib/*.md`) for each raw session archive (`archives/raw/*.jsonl`). Projections include:
    * Timeline table with UTC + local timestamps
    * Conversation summaries (user queries / assistant responses)
    * Tool activity with contextual stitching (toolUse → toolResult coupling)
    * Pre-emptive noise filtering (`NO_REPLY`, process poll chatter, repetitive status echoes)
    * Keywords, topics, and compaction anchors
    * Natural language time markers for improved semantic recall
    * Side-effect priority classification for tool entries
3.  **Tiered Distillation Pipeline**:
    *   **Phase 1 (Raw Distillation)**: Automatically distills archive projection markdown (`archives/mlib/*.md`) into daily logs (`memory/YYYY-MM-DD.md`) using cost-effective model tiers.
    *   **Librarian Optimizations**: semantic de-duplication keeps final-state conclusions, and optional topic discovery (`distill.topic_discovery=true` in `moon.toml`) maintains a top-of-file entity anchor block in each daily memory file.
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

1. History search backend is QMD collection `history`, rooted at `$MOON_ARCHIVES_DIR`, mask `mlib/**/*.md` (archive projections in `$MOON_ARCHIVES_DIR/mlib/*.md`).
2. Default history retrieval command is `MOON moon-recall --name history --query "<user-intent-query>"`. (If running from source instead of a compiled binary, use `cargo run --manifest-path /path/to/MOON/Cargo.toml -- moon-recall --name history --query "<user-intent-query>"`).
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
   `MOON verify --strict` (or `cargo run -- verify --strict`)
3. Check Moon runtime paths:
   `MOON moon-status` (or `cargo run -- moon-status`)
4. Run one watcher cycle:
   `MOON moon-watch --once` (or `cargo run -- moon-watch --once`)
5. Enable daemon mode only after one-shot run is clean.

## Quick start

```bash
cp .env.example .env
cp moon.toml.example moon.toml
$EDITOR .env
cargo install --path .
MOON verify --strict
MOON moon-status
```

`.env.example` and `moon.toml.example` are templates. Keep them generic; put
machine-specific values in `.env` and local `moon.toml` only.

Required `.env` value:

```bash
# Required: OpenClaw binary path (no default)
OPENCLAW_BIN=/absolute/path/to/openclaw
```

Default path profile (already set in `.env.example`):

```bash
# Binaries
# QMD is an external dependency (separate repo/project). Moon only calls its CLI.
QMD_BIN=$HOME/.bun/bin/qmd
QMD_DB=$HOME/.cache/qmd/index.sqlite

# Moon runtime paths
MOON_HOME=$HOME/MOON
MOON_ARCHIVES_DIR=$MOON_HOME/archives
MOON_MEMORY_DIR=$MOON_HOME/memory
MOON_MEMORY_FILE=$MOON_HOME/MEMORY.md
MOON_LOGS_DIR=$MOON_HOME/MOON/logs
MOON_CONFIG_PATH=$MOON_HOME/MOON/moon.toml

# OpenClaw session source
OPENCLAW_STATE_DIR=$HOME/.openclaw
OPENCLAW_CONFIG_PATH=$OPENCLAW_STATE_DIR/openclaw.json
OPENCLAW_SESSIONS_DIR=$HOME/.openclaw/agents/main/sessions
```

Workspace-root path profile (optional):

Use this if you want MOON runtime data under an existing workspace root instead of `$HOME/MOON`.

```bash
MOON_HOME=/path/to/workspace
MOON_ARCHIVES_DIR=$MOON_HOME/archives
MOON_MEMORY_DIR=$MOON_HOME/memory
MOON_MEMORY_FILE=$MOON_HOME/MEMORY.md
MOON_LOGS_DIR=$MOON_HOME/skills/MOON/logs
MOON_CONFIG_PATH=$MOON_HOME/skills/MOON/moon.toml
```

`moon.toml` is optional. If `MOON_CONFIG_PATH` points to a missing file, MOON
continues with built-in defaults plus `.env` overrides.

Recommended split:

1. `.env`: paths, binaries, provider/model/API keys, and env-only runtime knobs.
2. `moon.toml`: tuning in `[thresholds]`, `[watcher]`, `[distill]`, `[retention]`, `[inbound_watch]`.

If the same tuning key appears in both places, `.env` wins.

Create a local config file:

```bash
cp moon.toml.example moon.toml
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

```toml
[thresholds]
trigger_ratio = 0.5

[watcher]
poll_interval_secs = 30
cooldown_secs = 60

[distill]
mode = "idle"
idle_secs = 360
max_per_cycle = 3
residential_timezone = "UTC"
topic_discovery = true

[retention]
active_days = 7
warm_days = 30
cold_days = 31
```

Env-only guardrails (keep these in `.env`):

```bash

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

Run a few basics (assuming `MOON` is installed in `$PATH`, otherwise prefix with `cargo run -- `):

```bash
MOON status
MOON install --dry-run
MOON install
MOON moon-status
```

## CLI

Binary name: `MOON`

It is strongly recommended to install the binary to your `$PATH` using `cargo install --path .` rather than relying on `cargo run -- <command>` in production scenarios. You only need to run `cargo install --path .` again if you modify the Rust source code or plugin assets.

```bash
MOON <command> [flags]
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
9. `moon-watch [--once|--daemon] [--distill-now]`
10. `moon-recall --query <text> [--name <collection>]`
11. `moon-distill --archive <path> [--session-id <id>] [--allow-large-archive]`
    - default: archives larger than `MOON_DISTILL_CHUNK_BYTES` are auto-distilled in chunks
    - `--allow-large-archive`: force single-pass distill above the chunk threshold

Exit codes:

1. `0` command completed with `ok=true`
2. `2` command completed with `ok=false`
3. `1` runtime/process error

### Local Development & Testing
If you are actively developing the MOON codebase or writing an AI agent that needs to run tests:

Running the background watcher daemon (`moon-watch --daemon`) via `cargo run` is explicitly blocked. This is a safety feature to prevent file-locking starvation and CPU spikes loop issues if the daemon restarts.

To test the daemon with unreleased local changes, you must compile the binary first and execute it directly:
```bash
cargo build
./target/debug/MOON moon-watch --daemon
```

## Common workflows

After OpenClaw upgrade:

```bash
MOON post-upgrade
```

If you upgraded from older builds, clean legacy macOS LaunchAgents to avoid
duplicate daemons or stale `/tmp/moon*system*.log` logs:

```bash
launchctl list | rg -i "moon.*system" || true
ls "$HOME/Library/LaunchAgents" | rg -i "moon.*system" || true
```

Archive and index latest session:

```bash
MOON moon-snapshot
MOON moon-index --name history
```

`moon-index` also normalizes older archive layout into `archives/raw/` and backfills missing projection markdown files before running QMD sync.

Recall prior context:

```bash
MOON moon-recall --name history --query "your query"
```

Run one watcher cycle:

```bash
MOON moon-watch --once
```

Idle distill selection order:

1. Distill waits until the latest archive has been idle for `distill.idle_secs`.
2. It then selects the oldest pending archive day first.
3. It distills projection markdown sidecars (`*.md`) for those archives, not raw `*.jsonl`.
4. It processes up to `max_per_cycle` archives from that day.

Daily distill selection order:

1. In `distill.mode = "daily"`, distill attempts once per residential day (`distill.residential_timezone`) after the latest archive is idle for `distill.idle_secs`.
2. It selects the oldest pending archive day first.
3. It distills projection markdown sidecars (`*.md`) for those archives.
4. Use `MOON moon-watch --once --distill-now` for manual immediate layer-2 runs.

Retention lifecycle windows:

1. Active (`<= active_days`): keep archives for fast debug/resume.
2. Warm (`active_days < age <= warm_days`): retained and indexed.
3. Cold candidate (`>= cold_days`): deleted only when a distill marker exists.

Archive layout:

1. `archives/ledger.jsonl`: archive ledger metadata.
2. `archives/raw/*.jsonl`: raw snapshot copy (full fidelity).
3. `archives/mlib/*.md`: noise-reduced projection indexed by QMD.

## Configuration

The CLI autoloads `.env` on startup (if present).

Start from:

1. `.env.example`
2. `moon.toml.example`

Most-used `.env` variables:

1. `OPENCLAW_BIN`
2. `QMD_BIN`
3. `MOON_HOME`
4. `MOON_CONFIG_PATH`
5. `OPENCLAW_SESSIONS_DIR`
6. `MOON_DISTILL_PROVIDER`
7. `MOON_DISTILL_MODEL`
8. `GEMINI_API_KEY` / `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `AI_API_KEY` (distill only)
9. `MOON_DISTILL_CHUNK_BYTES` (default `auto`; use numeric bytes to force a fixed threshold)
10. `MOON_DISTILL_MAX_CHUNKS` (default `128`)
11. `MOON_DISTILL_MODEL_CONTEXT_TOKENS` (optional context hint used by `MOON_DISTILL_CHUNK_BYTES=auto`)
12. `MOON_HIGH_TOKEN_ALERT_THRESHOLD` (default `1000000`; set `0` to disable)
13. `MOON_ENABLE_COMPACTION_WRITE`
14. `MOON_ENABLE_SESSION_ROLLOVER`

Primary tuning belongs in `moon.toml`:

1. `[thresholds] trigger_ratio`
2. `[watcher] poll_interval_secs`, `cooldown_secs`
3. `[distill] mode`, `idle_secs`, `max_per_cycle`, `residential_timezone`, `topic_discovery`
4. `[retention] active_days`, `warm_days`, `cold_days`
5. `[inbound_watch] enabled`, `recursive`, `watch_paths`, `event_mode`

Legacy compatibility: `MOON_THRESHOLD_COMPACTION_RATIO`,
`MOON_THRESHOLD_ARCHIVE_RATIO`, and `MOON_THRESHOLD_PRUNE_RATIO` are still read
as fallback inputs for `MOON_TRIGGER_RATIO`.

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
# Stop known MOON service names
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.MOON.agent.plist 2>/dev/null || true
systemctl --user stop MOON 2>/dev/null || true
systemctl --user disable MOON 2>/dev/null || true

rm -f ~/Library/LaunchAgents/com.MOON.agent.plist
rm -f ~/.config/systemd/user/MOON.service
systemctl --user daemon-reload 2>/dev/null || true

OPENCLAW_STATE_DIR="${OPENCLAW_STATE_DIR:-$HOME/.openclaw}"
OPENCLAW_CONFIG_PATH="${OPENCLAW_CONFIG_PATH:-$OPENCLAW_STATE_DIR/openclaw.json}"
openclaw plugins uninstall MOON 2>/dev/null || true
rm -rf "$OPENCLAW_STATE_DIR/extensions/MOON"

MOON_HOME="${MOON_HOME:-$HOME/MOON}"
rm -rf "$MOON_HOME/archives" "$MOON_HOME/continuity" "$MOON_HOME/state" "$MOON_HOME/memory"
rm -rf "$MOON_HOME/MOON/logs"
rm -f "$MOON_HOME/MEMORY.md"

# Optional: remove persisted Moon config if you created one
rm -f "$MOON_HOME/MOON/moon.toml"
```

Note: uninstalling the plugin does not automatically restore custom OpenClaw
config values previously written under `plugins.entries.MOON` or
`agents.defaults.*`. Remove or revert those keys manually in
`$OPENCLAW_CONFIG_PATH` (default: `$OPENCLAW_STATE_DIR/openclaw.json`) if you want a full config rollback.
