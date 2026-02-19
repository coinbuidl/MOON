# Moon System Skill

Use this skill for Moon System operations:
1. Plugin lifecycle (`install`, `verify`, `repair`, `post-upgrade`).
2. Moon workflows (`moon-watch`, `moon-snapshot`, `moon-index`, `moon-recall`, `moon-distill`).

## Operating Rule

1. Use `README.md` in this repository as the source of truth for setup, env vars, commands, safety flags, and uninstall.
2. Run commands from the repo folder.
3. Prefer JSON mode for automation: `cargo run -- --json <command>`.
