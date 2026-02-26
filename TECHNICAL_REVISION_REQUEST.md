# MOON Technical Summary: Capability Blocks & Design Revision Needed

**Date**: 2026-02-27
**Status**: Degraded (Manual Embedding Blocked)
**Reporter**: Lilac (Main Agent)

## 1. The Core Issue: "Capability Missing" Block
During manual embedding "sprints" intended to clear the 70,000+ chunk backlog, the MOON binary is refusing execution due to an internal capability check failure.

### Error Trace
```json
{
  "command": "moon-embed",
  "ok": false,
  "details": [
    "state_file=/Users/lilac/.lilac_metaflora/moon/state/moon_state.json"
  ],
  "issues": [
    "embed capability missing: embed-help-no-max-docs"
  ]
}
```

### Observation
The `--help` menu explicitly lists `--max-docs <MAX_DOCS> [default: 25]`, but the implementation logic triggers a hard block when used. This prevents manual progress on the 398 pending documents (approx. 72,000 chunks).

## 2. Infrastructure & Environment
- **MOON_HOME**: `/Users/lilac/.lilac_metaflora`
- **OPENCLAW_BIN**: `/Users/lilac/.nvm/versions/node/v24.13.0/bin/openclaw`
- **Dependency**: `qmd` vector engine (Bun-based).
- **Watcher Status**: Running as daemon; successfully performing L1 Compaction and L2 Distillation, but embedding is stalled/drip-feeding too slowly.

## 3. Required Design Revisions
Master (Brian) requests a revision of the internal design to address the following:

1. **Resolve Capability Blocks**: Ensure that flags listed in the CLI (specifically `--max-docs` and bounded sprints) are actually authorized and functional without internal "capability missing" gatekeeping.
2. **Batch Stability**: Improve the robustness of the embedding logic for large document sets to prevent the "process stalls" seen in previous `--allow-unbounded` runs.
3. **Transparency**: Provide clearer diagnostic output when a capability or permission is missing, rather than a generic "degraded" status in the audit log.
4. **Sub-agent Protocol**: Formalize the `SKILL_SUBAGENT.md` (Read-only Recall) as the standard way for external agents to interface with the library without requiring system-level permissions.

## 4. Current Backlog
- **Pending Documents**: 398
- **Embedded Vectors**: 4,428
- **Target Chunks**: ~73,000+

Please review the `src/` logic regarding `capability` checks and the `moon-embed` subcommand to unlock manual sprint functionality.

---
*Signed, Lilac* ✨💕🎙️
