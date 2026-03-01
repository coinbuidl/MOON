# M.O.O.N.
> **Strategic Memory Augmentation & Context Distillation System**

### <span style="font-family:'Orbitron','Bank Gothic','Eurostile',sans-serif;"><font color="#dd0000">M</font>emory</span>
### <span style="font-family:'Orbitron','Bank Gothic','Eurostile',sans-serif;"><font color="#dd0000">O</font>ptimisation</span>
### <span style="font-family:'Orbitron','Bank Gothic','Eurostile',sans-serif;"><font color="#dd0000">O</font>rganisation</span>
### <span style="font-family:'Orbitron','Bank Gothic','Eurostile',sans-serif;"><font color="#dd0000">N</font>ode</span>

---

## Tactical Overview
**M.O.O.N.** is a high-performance, background-active memory optimiser designed to enhance AI systems with autonomous memory management. Like a tactical drone deployed in the heat of battle, it monitors, archives, and distills overwhelming context streams into high-signal structural intelligence.

It optimizes the **OpenClaw** context window by minimizing token usage while ensuring the agent retains seamless retrieval of historical knowledge.

## Core Features

1.  **Automated Lifecycle Watcher**: Monitors OpenClaw session and context size in real-time. Upon reaching defined thresholds, it triggers archiving, indexing, and compaction to prevent prompt overflow and minimize API costs.
    * During compaction, moon writes a deterministic `[MOON_ARCHIVE_INDEX]` note into the active session so agents can locate pre-compaction archives.
2.  **Semantic Context Retrieval**: moon writes a structured v2 markdown projection (`archives/mlib/*.md`) for each raw session archive (`archives/raw/*.jsonl`). Projections include:
    * Timeline table with UTC + local timestamps
    * Conversation summaries (user queries / assistant responses)
    * Tool activity with contextual stitching (`toolUse -> toolResult` coupling)
    * Pre-emptive noise filtering (`NO_REPLY`, process poll chatter, repetitive status echoes)
    * Keywords, topics, and compaction anchors
    * Natural language time markers for improved semantic recall
    * Side-effect priority classification for tool entries
3.  **Two-Layer Memory Pipeline**:
    *   **L1 Normalisation (`distill -mode norm`)**: deterministic filtering/normalisation from projection markdown (`archives/mlib/*.md`) into daily logs (`memory/YYYY-MM-DD.md`) without LLM summarisation.
    *   **L2 Synthesis (`distill -mode syns`)**: model-driven synthesis that rewrites `memory.md` from selected source files.
    *   **Source control for synthesis**: default is `today + memory.md`; explicit `-file` inputs synthesize only those files.
4.  **Embed Lifecycle Management**:
    * Manual command: `moon moon-embed --name history --max-docs 25`
    * Capability negotiation against installed QMD (`bounded` required; otherwise treated as missing/degraded)
    * Single-flight lock (`$MOON_LOGS_DIR/moon-embed.lock`) to avoid overlapping embed workers
    * Watcher embed runs automatically after compaction/L1 stages and before daily `syns`, then continues on cooldown-driven cycles
    * Bounded-only execution (`--max-docs`): no unbounded fallback path

## Recommended Agent Integration

To ensure reliable long-term memory and optimal token hygiene, it is recommended to explicitly define the boundary between the **M.O.O.N.** (automated) and the **Agent** (strategic) within your workspace rules (e.g., `AGENTS.md`):

*   **M.O.O.N. (Automated Lifecycle)**: Handles token compaction, short-term session state maintenance, L1 Normalisation to daily memory, and L2 Synthesis to `memory.md`.
*   **Agent (Strategic Review)**: Audits memory quality, adjusts prompts/rules, and curates long-term memory direction.

This modular architecture prevents the Agent from being overwhelmed by raw session data while ensuring that distilled knowledge is persisted with high signal-to-noise ratios.

### Skill Placement (Admin vs Sub-agent)

Keep both skill source files in this repo root:

1. `SKILL.md` for admin/operator tasks (`install`, `verify`, `repair`, watcher lifecycle).
2. `SKILL_SUBAGENT.md` for least-privilege sub-agent tasks (`moon-recall`, `distill`, bounded `moon-embed`).

If your runtime expects installed skills at `$CODEX_HOME/skills/<name>/SKILL.md`,
copy them as:

```bash
MOON_REPO="/absolute/path/to/moon"
SKILLS_HOME="${CODEX_HOME:-$HOME/.codex}/skills"

mkdir -p "$SKILLS_HOME/moon-admin" "$SKILLS_HOME/moon-subagent"
cp "$MOON_REPO/SKILL.md" "$SKILLS_HOME/moon-admin/SKILL.md"
cp "$MOON_REPO/SKILL_SUBAGENT.md" "$SKILLS_HOME/moon-subagent/SKILL.md"
```

Recommended role split:

1. Primary/operator agent: `moon-admin`.
2. Sub-agents: `moon-subagent` only.

### AGENTS.md Recall Policy Template

Add this block to your workspace `AGENTS.md` (adjust the repo path if different):

```md
### moon Archive Recall Policy (Required)

1. History search backend is QMD collection `history`, rooted at `$MOON_ARCHIVES_DIR`, mask `mlib/**/*.md` (archive projections in `$MOON_ARCHIVES_DIR/mlib/*.md`).
2. Default history retrieval command is `moon moon-recall --name history --query "<user-intent-query>"`. (If running from source instead of a compiled binary, use `cargo run --manifest-path /path/to/moon/Cargo.toml -- moon-recall --name history --query "<user-intent-query>"`).
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

1. Set `.env` (at minimum: ensure `openclaw` is on `PATH`; optional: set `OPENCLAW_BIN`; recommended: explicit path block below).
2. Apply plugin install + provenance self-heal:
   `moon install` (or `cargo run -- install`)
3. Validate environment and plugin wiring:
   `moon verify --strict` (or `cargo run -- verify --strict`)
4. Check moon runtime paths:
   `moon moon-status` (or `cargo run -- moon-status`)
5. Check daemon/state health:
   `moon moon-health` (or `cargo run -- moon-health`)
6. Inspect resolved runtime config:
   `moon config --show` (or `cargo run -- config --show`)
7. Run one watcher cycle:
   `moon moon-watch --once` (or `cargo run -- moon-watch --once`)
8. Enable daemon mode only after one-shot run is clean.
9. Install role-scoped skills (`moon-admin`, `moon-subagent`) if your runtime uses `$CODEX_HOME/skills`.

## Quick start

```bash
cp .env.example .env
cp moon.toml.example moon.toml
$EDITOR .env
cargo install --path .
moon install
moon verify --strict
moon moon-status
moon moon-health
moon config --show
```

`.env.example` and `moon.toml.example` are templates. Keep them generic; put
machine-specific values in `.env` and local `moon.toml` only.

Workspace model (agent-facing):

1. `MOON_HOME` is the workspace root for moon runtime data.
2. Recommended default: `MOON_HOME=$HOME` (so home is the workspace root).
3. Repo path should be `MOON_HOME/moon`.
4. Daily memory path is `MOON_HOME/memory/YYYY-MM-DD.md`.

`.env` autoload precedence:

1. Standard dotenv search from current working directory upward.
2. Deterministic moon repo fallback:
   - `MOON_HOME/moon/.env`
   - if `MOON_HOME` is unset: `$HOME/moon/.env`

This makes daemon runs resilient when started outside the moon repo working
directory.

Agent check: ensure `.env` exists in the moon repo folder (`moon/.env`).
If `.env` is missing at startup, moon logs a warning and continues in
non-distill/non-embed mode.

Workspace boundary safety:

1. Mutating commands validate CWD against the daemon-recorded workspace (or explicit `MOON_HOME` when no daemon lock is present).
2. Diagnostic commands (`status`, `moon-status`, `moon-health`, `verify`, `config`) are always allowed from any directory.
3. Escape hatch: pass global `--allow-out-of-bounds` to bypass CWD enforcement.

OpenClaw binary resolution:

```bash
# Preferred: ensure `openclaw` is available on PATH.
# Optional override: pin an explicit binary path.
OPENCLAW_BIN=/absolute/path/to/openclaw
```

Default path profile (already set in `.env.example`):

```bash
# Binaries
# QMD is an external dependency (separate repo/project). moon only calls its CLI.
QMD_BIN=$HOME/.bun/bin/qmd
QMD_DB=$HOME/.cache/qmd/index.sqlite

# moon runtime paths
MOON_HOME=$HOME
MOON_ARCHIVES_DIR=$MOON_HOME/archives
MOON_MEMORY_DIR=$MOON_HOME/memory
MOON_MEMORY_FILE=$MOON_HOME/MEMORY.md
MOON_LOGS_DIR=$MOON_HOME/moon/logs
MOON_CONFIG_PATH=$MOON_HOME/moon/moon.toml
MOON_STATE_FILE=$MOON_HOME/moon/state/moon_state.json

# OpenClaw session source
OPENCLAW_STATE_DIR=$HOME/.openclaw
OPENCLAW_CONFIG_PATH=$OPENCLAW_STATE_DIR/openclaw.json
OPENCLAW_SESSIONS_DIR=$HOME/.openclaw/agents/main/sessions
```

Workspace-root path profile (optional):

Use this if your workspace root is not `$HOME`.

```bash
MOON_HOME=/path/to/workspace
MOON_ARCHIVES_DIR=$MOON_HOME/archives
MOON_MEMORY_DIR=$MOON_HOME/memory
MOON_MEMORY_FILE=$MOON_HOME/MEMORY.md
MOON_LOGS_DIR=$MOON_HOME/moon/logs
MOON_CONFIG_PATH=$MOON_HOME/moon/moon.toml
MOON_STATE_FILE=$MOON_HOME/moon/state/moon_state.json
```

`moon.toml` is optional. If `MOON_CONFIG_PATH` points to a missing file, moon
continues with built-in defaults plus `.env` overrides.

State path override precedence:

1. `MOON_STATE_FILE` (exact file path)
2. `MOON_STATE_DIR` (directory; file becomes `moon_state.json`)
3. fallback: `$MOON_HOME/moon/state/moon_state.json`

Recommended split:

1. `.env`: paths, binaries, provider/model/API keys, and env-only runtime knobs.
2. `moon.toml`: tuning in `[context]`, `[watcher]`, `[distill]`, `[retention]`, `[embed]`, `[inbound_watch]` (and optional legacy `[thresholds]`).

If the same tuning key appears in both places, `.env` wins.

Create a local config file:

```bash
cp moon.toml.example moon.toml
```

Context policy (optional but recommended when moon owns compaction):

```toml
[context]
window_mode = "inherit"            # or "fixed" with window_tokens
# window_tokens = 200000
prune_mode = "disabled"            # "disabled" or "guarded"
compaction_authority = "moon"      # "moon" or "openclaw"
compaction_start_ratio = 0.78
compaction_emergency_ratio = 0.90
```

When `compaction_authority = "moon"`:

1. `moon install` / `moon repair` enforce OpenClaw compaction mode to `default` (valid on current OpenClaw builds).
2. moon watcher is the primary trigger for `/compact` based on `[context]` ratios.
3. Simplified compaction loop: if usage is still `>= compaction_start_ratio` after cooldown, moon can compact again on the next eligible cycle.
4. Emergency ratio can bypass cooldown (`usage >= compaction_emergency_ratio`).
5. OpenClaw may still auto-compact as a fallback on overflow/threshold paths.
6. `moon status` reports a policy violation (`ok=false`) if OpenClaw config drifts from the expected mode for the selected authority.

Synthesis model profile (recommended for the agent):

```bash
# `norm` uses no LLM.
# LLM calls are used only by `syns`.
# Recommend a high-reasoning model for better durable memory quality.
MOON_WISDOM_PROVIDER=openai
MOON_WISDOM_MODEL=gpt-4.1
OPENAI_API_KEY=...

# Alternative high-reasoning options:
# MOON_WISDOM_PROVIDER=anthropic
# MOON_WISDOM_MODEL=claude-3-7-sonnet-latest
#
# MOON_WISDOM_PROVIDER=gemini
# MOON_WISDOM_MODEL=gemini-2.5-pro
```

Distill safety guardrails (recommended):

```toml
[context]
window_mode = "inherit"
prune_mode = "disabled"
compaction_authority = "moon"
compaction_start_ratio = 0.78
compaction_emergency_ratio = 0.90

[watcher]
poll_interval_secs = 30
cooldown_secs = 60

[distill]
mode = "daily"
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

# Optional explicit context-window hints for `syns` large-file chunk planning.
# If unset, moon auto-detects/infers context window per provider/model.
# MOON_WISDOM_CONTEXT_TOKENS=200000

```

Cheapest possible mode (zero API cost, local-only synthesis):

```bash
MOON_WISDOM_PROVIDER=local
```

Run a few basics (assuming `moon` is installed in `$PATH`, otherwise prefix with `cargo run -- `):

```bash
moon status
moon install --dry-run
moon install
moon verify --strict
moon moon-status
```

## CLI

Binary name: `moon`

It is strongly recommended to install the binary to your `$PATH` using `cargo install --path .` rather than relying on `cargo run -- <command>` in production scenarios. You only need to run `cargo install --path .` again if you modify the Rust source code or plugin assets.

```bash
moon <command> [flags]
```

Global flag:

1. `--json` outputs machine-readable `CommandReport`
2. `--allow-out-of-bounds` bypasses workspace CWD lock checks for mutating commands

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
9. `moon-watch [--once|--daemon] [--distill-now] [--dry-run]`
10. `moon-stop`
11. `moon-embed [--name <collection>] [--max-docs <N>] [--dry-run] [--watcher-trigger]`
12. `moon-recall --query <text> [--name <collection>]`
13. `distill -mode <norm|syns> [-archive <path>] [-session-id <id>] [-file <path> ...] [-dry-run]`
    - `-mode norm` (default): L1 Normalisation for one projection file (`archives/mlib/*.md`) into daily memory
    - `-mode syns`: L2 Synthesis rewrites the whole `memory.md` from synthesis output
    - `-mode syns` default sources (manual CLI): today's daily memory + current `memory.md`
    - `-mode syns -file <path> ...`: distill only those files together; `memory.md` participates only if explicitly included as a `-file`
    - `moon-distill` remains as a backward-compatible alias
14. `config [--show]`
15. `moon-health`

Exit codes:

1. `0` command completed with `ok=true`
2. `2` command completed with `ok=false`
3. `1` runtime/process error

## Provenance Behavior (Agent-critical)

1. `moon install` always normalizes `plugins.installs.moon` (`source`, `sourcePath`, `installPath`) to the managed plugin directory.
2. `moon status` and `moon verify --strict` treat OpenClaw runtime diagnostics from `openclaw plugins list --json` as the authoritative provenance signal.
3. If runtime diagnostics report `loaded without install/load-path provenance`, `status`/`verify --strict` fail hard.
4. If `plugins.installs.moon` is missing or path-mismatched but runtime diagnostics are clean, `status` prints a non-fatal `provenance repair hint`.
5. First-time bootstrap and post-upgrade routine should always include `moon install` before `moon verify --strict`.

### Local Development & Testing
If you are actively developing the moon codebase or writing an AI agent that needs to run tests:

Running the background watcher daemon (`moon-watch --daemon`) via `cargo run` is explicitly blocked. This is a safety feature to prevent file-locking starvation and CPU spikes loop issues if the daemon restarts.

To test the daemon with unreleased local changes, you must compile the binary first and execute it directly:
```bash
cargo build
./target/debug/moon moon-watch --daemon
```

## Common workflows

After OpenClaw upgrade:

```bash
moon post-upgrade
```

If you upgraded from older builds, clean legacy macOS LaunchAgents to avoid
duplicate daemons or stale `/tmp/moon*system*.log` logs:

```bash
launchctl list | rg -i "moon.*system" || true
ls "$HOME/Library/LaunchAgents" | rg -i "moon.*system" || true
```

Archive and index latest session:

```bash
moon moon-snapshot
moon moon-index --name history
```

`moon-index` also normalizes older archive layout into `archives/raw/` and backfills missing projection markdown files before running QMD sync.

Run manual embed sprint:

```bash
moon moon-embed --name history --max-docs 25
```

Recall prior context:

```bash
moon moon-recall --name history --query "your query"
```

Run one watcher cycle:

```bash
moon moon-watch --once
```

Dry-run watcher planning cycle (no mutation/state writes):

```bash
moon moon-watch --once --dry-run
```

Stop the watcher daemon:

```bash
moon moon-stop
```

Health probe:

```bash
moon moon-health
```

Idle distill selection order:

1. L1 Normalisation waits until the latest archive has been idle for `distill.idle_secs`.
2. It then selects the oldest pending archive day first.
3. It distills projection markdown sidecars (`*.md`) for those archives, not raw `*.jsonl`.
4. It processes up to `max_per_cycle` archives from that day.

Daily `syns` schedule:

1. Watcher attempts `syns` once per residential day (`distill.residential_timezone`) on the first cycle after local midnight.
2. Auto `syns` sources are yesterday's daily file (`memory/YYYY-MM-DD.md`) plus current `memory.md` (when present).
3. It does not require `distill.mode = manual`; agents can run `moon distill -mode syns` directly at any time.
4. `moon moon-watch --once --distill-now` remains a manual trigger for L1 queue processing.

Retention lifecycle windows:

1. Active (`<= active_days`): keep archives for fast debug/resume.
2. Warm (`active_days < age <= warm_days`): retained and indexed.
3. Cold candidate (`>= cold_days`): deleted only when a distill marker exists.

Embed lifecycle windows:

1. Watcher embed is always auto (legacy `embed.mode` values normalize to `auto`).
2. Watcher attempts embed after compaction/L1 stages and before daily `syns` when `syns` is due.
3. Watcher execution is gated by `embed.cooldown_secs` and `embed.min_pending_docs`.
4. Manual `moon-embed` runs immediately and bypasses watcher cooldown gating.
5. Manual `moon-embed` does not reset the watcher cooldown clock.
6. QMD must support bounded embed (`--max-docs`); otherwise watcher degrades and manual embed returns capability-missing.
7. `embed.idle_secs` is retained only for compatibility and does not gate watcher embed execution.
8. Lock behavior is non-blocking: watcher embed skips current cycle when lock is busy; manual embed returns lock error (no wait queue).

Archive layout:

1. `archives/ledger.jsonl`: archive ledger metadata.
2. `archives/raw/*.jsonl`: raw snapshot copy (full fidelity).
3. `archives/mlib/*.md`: noise-reduced projection indexed by QMD.

## Configuration

The CLI autoloads `.env` on startup when available. If no `.env` is found, moon
logs a warning and continues with defaults/explicit env vars.

Start from:

1. `.env.example`
2. `moon.toml.example`

Most-used `.env` variables:

1. `OPENCLAW_BIN` (optional override; `openclaw` is auto-resolved from `PATH` when unset)
2. `QMD_BIN`
3. `MOON_HOME`
4. `MOON_CONFIG_PATH`
5. `MOON_STATE_FILE` / `MOON_STATE_DIR`
6. `OPENCLAW_SESSIONS_DIR`
7. `MOON_WISDOM_PROVIDER` (primary provider selector for `distill -mode syns`)
8. `MOON_WISDOM_MODEL` (primary model selector for `syns`)
9. `MOON_WISDOM_CONTEXT_TOKENS` (optional context-window hint for large-file chunk planning in `syns`)
10. `GEMINI_API_KEY` / `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `AI_API_KEY` (for `syns`)
11. `MOON_ENABLE_COMPACTION_WRITE`
12. `MOON_ENABLE_SESSION_ROLLOVER`
13. `MOON_EMBED_MODE` (`auto`; legacy aliases `idle` and `manual` normalize to `auto`)
14. `MOON_EMBED_IDLE_SECS` (legacy compatibility knob; no watcher gate effect)
15. `MOON_EMBED_COOLDOWN_SECS`
16. `MOON_EMBED_MAX_DOCS_PER_CYCLE`
17. `MOON_EMBED_MIN_PENDING_DOCS`
18. `MOON_EMBED_MAX_CYCLE_SECS`
19. `MOON_HEALTH_MAX_CYCLE_AGE_SECS` (health freshness threshold; default `600`)

Config hardening behaviors:

1. Unknown `MOON_*` variables are warned on startup, with typo suggestions when close matches exist.
2. `moon config --show` prints fully resolved config values (defaults -> `moon.toml` -> env overrides).
3. Secret env values are masked in diagnostics (`moon-status`, `config --show`).

Primary tuning belongs in `moon.toml`:

1. `[context] window_mode`, `window_tokens`, `prune_mode`, `compaction_authority`, `compaction_start_ratio`, `compaction_emergency_ratio`
2. `[watcher] poll_interval_secs`, `cooldown_secs`
3. `[distill] mode`, `idle_secs`, `max_per_cycle`, `residential_timezone`, `topic_discovery`
4. `[retention] active_days`, `warm_days`, `cold_days`
5. `[embed] mode` (fixed `auto`; legacy aliases normalize), `idle_secs` (legacy compatibility), `cooldown_secs`, `max_docs_per_cycle`, `min_pending_docs`, `max_cycle_secs`
6. `[inbound_watch] enabled`, `recursive`, `watch_paths`, `event_mode`
7. `[thresholds] trigger_ratio` (legacy/fallback path when context policy is not active)

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

This removes moon services/plugin/runtime files and keeps user assets intact.

User assets that are preserved:

1. `$MOON_ARCHIVES_DIR` (archives)
2. `$MOON_MEMORY_DIR` (daily memory)
3. `$MOON_MEMORY_FILE` (long-term memory)

Use trash-first cleanup (preferred):

```bash
trash_path() {
  [ -e "$1" ] || return 0
  if command -v trash >/dev/null 2>&1; then
    trash "$1"
  elif command -v gio >/dev/null 2>&1; then
    gio trash "$1"
  else
    mkdir -p "$HOME/.Trash"
    mv "$1" "$HOME/.Trash/"
  fi
}

# Stop known moon service names
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.moon.agent.plist 2>/dev/null || true
systemctl --user stop moon 2>/dev/null || true
systemctl --user disable moon 2>/dev/null || true

trash_path "$HOME/Library/LaunchAgents/com.moon.agent.plist"
trash_path "$HOME/.config/systemd/user/moon.service"
systemctl --user daemon-reload 2>/dev/null || true

OPENCLAW_STATE_DIR="${OPENCLAW_STATE_DIR:-$HOME/.openclaw}"
OPENCLAW_CONFIG_PATH="${OPENCLAW_CONFIG_PATH:-$OPENCLAW_STATE_DIR/openclaw.json}"
openclaw plugins uninstall moon 2>/dev/null || true
trash_path "$OPENCLAW_STATE_DIR/extensions/moon"

MOON_HOME="${MOON_HOME:-$HOME}"
# Remove moon-owned runtime artifacts only (keep archives/memory/MEMORY.md)
trash_path "$MOON_HOME/continuity"
trash_path "$MOON_HOME/moon/state"
trash_path "$MOON_HOME/state"                # legacy state location
trash_path "$MOON_HOME/moon/logs"
[ -n "${MOON_STATE_FILE:-}" ] && trash_path "$MOON_STATE_FILE"
[ -n "${MOON_STATE_DIR:-}" ] && trash_path "$MOON_STATE_DIR"

# Optional: remove persisted moon config if you created one
trash_path "$MOON_HOME/moon/moon.toml"
```

Note: uninstalling the plugin does not automatically restore custom OpenClaw
config values previously written under `plugins.entries.moon` or
`agents.defaults.*`. Remove or revert those keys manually in
`$OPENCLAW_CONFIG_PATH` (default: `$OPENCLAW_STATE_DIR/openclaw.json`) if you want a full config rollback.
