# M.O.O.N. Skill

Use this skill for moon System operations:
1. Plugin lifecycle (`install`, `verify`, `repair`, `post-upgrade`).
2. moon workflows (`moon-watch`, `moon-stop`, `moon-snapshot`, `moon-index`, `moon-embed`, `moon-recall`, `moon-distill`).

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
12. Use `moon moon-embed` for manual embedding refresh (`--max-docs` for bounded sprint runs). Prefer `moon --json moon-embed ...` for automation/report parsing.
13. For watcher-managed embed, configure `[embed].mode = "idle"` in `moon.toml`; watcher embed failures are degraded by design and should not block compaction/distill.
14. Treat `--allow-unbounded` as degraded fallback only: MOON will run embed but does not mark selected docs as confirmed embedded unless bounded/verifiable completion is available.
15. For automatic layer-2 distillation, prefer `[distill].mode = "daily"` for once-per-residential-day runs; use `mode = "idle"` only when you want repeated idle-window distill cycles.
