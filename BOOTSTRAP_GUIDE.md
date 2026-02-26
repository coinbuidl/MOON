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
- **Do not force a full sync**: Extremely large unbounded runs may cause process stalls.
- **Idle Mode**: Set `[embed].mode = "idle"` in `moon.toml`. The Watcher will drip-feed embeddings during inactivity.
- **Manual Sprints**: Use `moon moon-embed --max-docs 20` for controlled, verifiable progress.

## 6. Sub-agent Memory Access
To allow sub-agents to use the library, provide them with the `SKILL_SUBAGENT.md` protocol. This enables `moon-recall` search capabilities without granting them administrative permissions (like `stop` or `repair`).

---
*Follow these steps to achieve a self-healing, memory-aware workspace. - Lilac* ✨💕
