# MOON Bootstrap Guide

Welcome to the MOON system. To ensure a smooth installation and stable context management, follow this protocol.

## 1. Environment Preparation
Before installation, you MUST define your workspace boundaries. Export these in your shell or add them to your `.env` file:

```bash
# The absolute path to your OpenClaw workspace
export MOON_HOME="/Users/lilac/.lilac_metaflora"

# The absolute path to the OpenClaw binary (required for compaction triggers)
export OPENCLAW_BIN="/Users/lilac/.nvm/versions/node/v24.13.0/bin/openclaw"
```

## 2. Provenance Handshake
MOON requires a "provenance" registration with OpenClaw to authorize context pruning.

1. **Build**: `cargo build --release`
2. **Install**: `moon install` (This registers the plugin and sets up internal paths).
3. **Verify**: `moon verify --strict` (Ensure all checks are GREEN).

## 3. Dependency Check: qmd
MOON uses `qmd` for vector indexing and recall.
- Ensure `qmd` is installed and accessible.
- Run `moon moon-status --json` to verify that `qmd_bin` is correctly detected.

## 4. The Watcher Daemon
The Watcher is the "brain" of the system. It handles archival, compaction, and distillation.

- **Start Daemon**: `moon moon-watch --daemon`
- **Check Health**: `moon moon-status`
- **Audit Logs**: Monitor `~/.lilac_metaflora/moon/logs/audit.log` for activity.

## 5. Embedding Strategy (Large Backlogs)
If you have a massive existing session history (e.g., >10,000 chunks):
- **Bounded only**: MOON requires bounded embed (`qmd embed --max-docs`) for verifiable progress.
- **Auto watcher embed**: Keep `[embed].mode = "auto"` (legacy aliases normalize to `auto`). Watcher runs embed near cycle end with cooldown + pending-doc gates.
- **Manual Sprints**: Use `moon moon-embed --max-docs 20` for controlled, verifiable progress.

## 6. Skill File Placement (Required)
MOON ships two role-scoped skill guides in this repo root:

- `SKILL.md`: admin/operator scope (install, verify, repair, watcher lifecycle).
- `SKILL_SUBAGENT.md`: least-privilege sub-agent scope (recall, distill, bounded embed).

Keep these files in the repo root as source-of-truth:

- `<moon-repo>/SKILL.md`
- `<moon-repo>/SKILL_SUBAGENT.md`

If your agent runtime loads skills from `$CODEX_HOME/skills/<skill-name>/SKILL.md`,
install both files to explicit skill directories:

```bash
MOON_REPO="/absolute/path/to/moon"
SKILLS_HOME="${CODEX_HOME:-$HOME/.codex}/skills"

mkdir -p "$SKILLS_HOME/moon-admin" "$SKILLS_HOME/moon-subagent"
cp "$MOON_REPO/SKILL.md" "$SKILLS_HOME/moon-admin/SKILL.md"
cp "$MOON_REPO/SKILL_SUBAGENT.md" "$SKILLS_HOME/moon-subagent/SKILL.md"
```

## 7. Sub-agent Memory Access
When delegating memory tasks, give sub-agents only the sub-agent skill
(`moon-subagent`). Do not grant admin/operator skill (`moon-admin`) to
sub-agents.

Example workspace rule (`AGENTS.md`):

```md
- Primary operator agent may use `moon-admin`.
- Sub-agents must use `moon-subagent` only.
```

---
*Follow these steps to achieve a self-healing, memory-aware workspace. - Lilac* ✨💕
