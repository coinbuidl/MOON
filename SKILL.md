# M.O.O.N. Admin Skill

Use this skill for moon system admin/operator operations.
For least-privilege sub-agents, use `SKILL_SUBAGENT.md` instead.

This skill covers:
1. Plugin lifecycle (`install`, `verify`, `repair`, `post-upgrade`).
2. moon workflows (`moon-watch`, `moon-stop`, `moon-snapshot`, `moon-index`, `moon-embed`, `moon-recall`, `distill`).

## Operating Rule

1. Use `README.md` in this repository as the source of truth for setup, env vars, commands, safety flags, and uninstall.
2. Always run from the repo root (or source/export the repo `.env` first). Path model: `MOON_HOME` is workspace root, repo path is `MOON_HOME/moon`, memory path is `MOON_HOME/memory`, and fallback dotenv path is `MOON_HOME/moon/.env` (or `$HOME/moon/.env` if `MOON_HOME` is unset).
3. If the `moon` binary is installed in your `$PATH` (e.g. `~/.cargo/bin/moon`), run `moon <command>`. Otherwise, run `cargo run -- <command>` from the repo folder.
4. If you modify any Rust source code (`src/*.rs`) or plugin assets (`assets/plugin/*`), you MUST run `cargo install --path .` ONCE to compile and apply those changes.
5. Prefer JSON mode for automation: `moon --json <command>` or `cargo run -- --json <command>`.
6. For first-time setup and after OpenClaw upgrades, run `moon install` before `moon verify --strict`. `install` is responsible for provenance self-heal (`plugins.installs.moon.*`).
7. Treat runtime provenance diagnostics as authoritative: if `moon status` or `moon verify --strict` reports `loaded without install/load-path provenance`, run `moon install` and re-check.
8. If `moon status` only prints `provenance repair hint` (without failing), it is non-fatal drift; run `moon install` to normalize.
9. If `[context].compaction_authority = "moon"` is configured in `moon.toml`, enforce OpenClaw `agents.defaults.compaction.mode = "default"` (valid mode) and let moon drive earlier compaction via `[context]` ratios.
10. On current OpenClaw versions, auto-compaction cannot be hard-disabled via config mode; treat moon as primary compaction orchestrator with OpenClaw fallback.
11. If `moon status` reports `context policy drift`, fix with `moon install` (or `moon repair`) and re-check before continuing.
12. Use `moon moon-embed` for manual embedding refresh (`--max-docs` bounded sprint runs). Manual runs trigger immediately and bypass watcher cooldown gates.
13. Watcher embed is always auto and runs after compaction/L1 stages and before daily `syns` when due. Gating is `[embed].cooldown_secs` + `[embed].min_pending_docs`; `[embed].idle_secs` is legacy compatibility only.
14. Manual embed must not alter watcher cooldown timing; watcher cooldown continues from watcher-trigger timestamps only.
15. Keep embed bounded-only. If QMD lacks `--max-docs`, watcher degrades and manual embed returns capability-missing (no unbounded fallback).
16. Embed lock is non-blocking: watcher reports degraded/locked and retries next cycle; manual command returns lock error immediately (no queue/wait behavior).
17. `[distill].mode` controls L1 Normalisation queue behavior (`daily` for once-per-day L1 queue attempts, `idle` for repeated idle-window L1 attempts). Auto L2 `syns` is a separate once-per-residential-day watcher trigger; manual `moon distill -mode syns` is always available.
