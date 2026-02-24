# M.O.O.N. Skill

Use this skill for moon System operations:
1. Plugin lifecycle (`install`, `verify`, `repair`, `post-upgrade`).
2. moon workflows (`moon-watch`, `moon-snapshot`, `moon-index`, `moon-recall`, `moon-distill`).

## Operating Rule

1. Use `README.md` in this repository as the source of truth for setup, env vars, commands, safety flags, and uninstall.
2. If the `moon` binary is installed in your `$PATH` (e.g. `~/.cargo/bin/moon`), run `moon <command>`. Otherwise, run `cargo run -- <command>` from the repo folder.
3. If you modify any Rust source code (`src/*.rs`) or plugin assets (`assets/plugin/*`), you MUST run `cargo install --path .` ONCE to compile and apply those changes.
4. Prefer JSON mode for automation: `moon --json <command>` or `cargo run -- --json <command>`.
