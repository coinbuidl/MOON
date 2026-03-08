# OpenClaw ContextEngine Research

Verified against the locally installed OpenClaw at:

- `openclaw --version` -> `2026.3.7`
- binary: `/Users/lilac/.nvm/versions/node/v24.13.0/bin/openclaw`
- package root: `/Users/lilac/.nvm/versions/node/v24.13.0/lib/node_modules/openclaw`

## Verdict

The ContextEngine system is real in local OpenClaw `2026.3.7`. It is not just a roadmap item.

OpenClaw now supports:

- a dedicated plugin kind: `context-engine`
- an exclusive plugin slot: `plugins.slots.contextEngine`
- a runtime registration API: `api.registerContextEngine(id, factory)`
- a full lifecycle contract for context management
- a built-in default engine: `legacy`

This means MOON can evolve from an external watcher/archiver into an active context orchestrator without patching OpenClaw core.

## Local Evidence

Confirmed in the local install:

- `CHANGELOG.md`
  - `2026.3.7` explicitly announces the `ContextEngine` plugin interface, the `contextEngine` slot, lifecycle hooks, `LegacyContextEngine`, and `sessions.get`.
- `docs/concepts/context.md`
  - says OpenClaw uses built-in `legacy` by default and delegates assembly/compaction/subagent context lifecycle to the selected context engine plugin.
- `docs/tools/plugin.md`
  - documents `plugins.slots.contextEngine`
  - documents `api.registerContextEngine(...)`
  - includes a minimal example plugin
- `docs/plugins/manifest.md`
  - confirms plugin manifest `kind: "context-engine"`
- `dist/plugin-sdk/context-engine/types.d.ts`
  - ships the actual TypeScript contract for the interface

## Confirmed Contract

### Manifest and config

A real context engine plugin needs:

- `openclaw.plugin.json`
- `kind: "context-engine"`
- a valid `configSchema`
- runtime registration via `api.registerContextEngine(...)`

It is activated through:

```json
{
  "plugins": {
    "slots": {
      "contextEngine": "moon-context-engine"
    }
  }
}
```

Config changes require a gateway restart.

### Interface

The local TypeScript definition exposes this lifecycle:

- `bootstrap`
- `ingest`
- `ingestBatch`
- `assemble`
- `compact`
- `afterTurn`
- `prepareSubagentSpawn`
- `onSubagentEnded`
- `dispose`

Important return shapes confirmed locally:

- `assemble` returns:
  - `messages`
  - `estimatedTokens`
  - optional `systemPromptAddition`
- `compact` returns:
  - `ok`
  - `compacted`
  - optional `reason`
  - optional structured `result`
- `info.ownsCompaction`
  - tells OpenClaw whether the engine owns compaction behavior

### Confirmed runtime workflow

OpenClaw resolves one active context engine for the `contextEngine` slot.

- default selection is built-in `legacy`
- selecting a plugin replaces the active engine for assembly/compaction lifecycle
- this is a slot replacement model, not a partial hook chain

The runtime flow for a normal turn is:

1. Resolve the active context engine from `plugins.slots.contextEngine`.
2. If the session already exists and the engine implements `bootstrap`, call it before the run.
3. OpenClaw prepares the normal session state, sanitizes history, validates/provider-normalizes turns, and applies its own transcript limits.
4. OpenClaw calls `assemble({ sessionId, messages, tokenBudget })`.
5. The engine may:
   - return a replacement `messages` array
   - return `systemPromptAddition` to prepend extra system-level context
6. OpenClaw sends the assembled context to the model.
7. After the turn, OpenClaw either:
   - calls `afterTurn(...)` if implemented, or
   - falls back to `ingestBatch(...)`, or
   - falls back to per-message `ingest(...)`

The runtime flow for compaction is:

1. `/compact` resolves the active context engine.
2. OpenClaw calls `compact({ sessionId, sessionFile, tokenBudget, ... })` on that engine.
3. Automatic overflow recovery also uses the selected engine's `compact(...)`.
4. If `info.ownsCompaction = true`, OpenClaw disables its internal Pi auto-compaction guard for that run.

Important nuance:

- `ownsCompaction = false` does not mean OpenClaw will magically keep a separate second engine active for compaction.
- once MOON is selected into the slot, MOON is the engine OpenClaw calls for `assemble`, `compact`, and post-turn lifecycle.
- therefore a Phase 1 MOON engine should be assemble-first in behavior, but still provide a safe `compact` path, most likely by delegating to `LegacyContextEngine`.

The built-in `legacy` engine is explicitly a wrapper around the current behavior:

- `ingest`: no-op
- `assemble`: pass-through
- `compact`: delegate to the existing compaction pipeline

That makes `LegacyContextEngine` the clean compatibility bridge for an early MOON engine.

## What This Means For MOON

### Immediate opportunity

The strongest near-term use is `assemble`.

MOON already has:

- historical archive recall
- daily memory
- long-term `MEMORY.md`
- embedding/index workflows

So a context engine can use `assemble` to inject relevant historical context directly into the model context, instead of relying on the operator or agent to manually run `moon recall`.

### Strong follow-up opportunity

`afterTurn` is the next best hook.

It can:

- ingest freshly completed turns into MOON-managed memory
- update durable decisions
- queue or trigger L1/L2 distillation work

This is more aligned with MOON’s existing architecture than replacing compaction immediately.

### Higher-risk opportunity

`compact` is powerful, but this is where complexity rises quickly.

MOON currently manages compaction externally through:

- watcher thresholds
- archive/projection generation
- index note writing
- continuity side effects

Replacing that with a ContextEngine too early would couple MOON tightly to OpenClaw’s runtime semantics before the simpler `assemble` path is proven.

## Recommended MOON Integration Path

Keep this simple.

### Phase 1

Build `moon-context-engine` as a separate plugin that is assemble-first, with minimal legacy delegation for required compatibility:

- manifest with `kind: "context-engine"`
- `registerContextEngine("moon-context-engine", factory)`
- `assemble`
- `compact` delegated to `LegacyContextEngine`
- minimal `ingest` compatibility behavior
- `info.ownsCompaction = false`

Behavior:

- leave OpenClaw `legacy` compaction behavior intact
- use MOON recall/indexed memory to inject a bounded “System Reference” block
- do not rewrite MOON watcher logic yet

### Phase 2

Add:

- `ingestBatch`
- `afterTurn`

Behavior:

- persist recent turn signals into MOON-owned memory inputs
- update or queue durable-memory synthesis
- keep actual compaction external for now

### Phase 3

Only if Phases 1-2 are stable, evaluate:

- `compact`
- `prepareSubagentSpawn`
- `onSubagentEnded`

This is the point where MOON could become the primary context orchestrator rather than a watcher around the outside.

## Constraints and Risks

### 1. MOON currently is not a context-engine plugin

Current MOON plugin assets are still a normal plugin:

- `assets/plugin/openclaw.plugin.json`
- `assets/plugin/index.js`

They do not declare `kind: "context-engine"` and do not register a context engine.

### 2. Current watcher logic still matters

Today MOON’s real behavior lives in Rust:

- archive flow
- distillation
- recall
- embed/index maintenance
- watcher-triggered compaction orchestration

A ContextEngine plugin would need a bridge into that Rust functionality or a deliberate TypeScript reimplementation of a thin subset.

### 3. Do not assume `compact` can just call existing MOON compaction

The local contract is real, but the operational semantics still need testing:

- when exactly `compact` runs
- what transcript mutations are safe
- how token budgets are passed
- how `ownsCompaction` affects OpenClaw’s built-in behavior

### 4. Subagent hooks are real, but still unproven for MOON

The local API includes:

- `prepareSubagentSpawn`
- `onSubagentEnded`

This is promising for scoped memory handoff, but MOON does not currently have an internal subagent-context protocol. This should not be the first implementation target.

## Practical Implementation Sketch

The minimum viable design is:

1. Add a new plugin directory, separate from the existing `moon` compaction plugin.
2. Register `moon-context-engine`.
3. Instantiate `LegacyContextEngine` inside the plugin and delegate `compact` to it.
4. In `assemble`, call a small bridge command such as:
   - `moon recall --name history --query "<derived query>"`
5. Convert top hits into:
   - appended assistant/system reference messages, or
   - `systemPromptAddition`
6. Keep injected context byte- and token-bounded.

This avoids rewriting the current Rust core while testing whether the hook is actually useful in real sessions.

## Local Research Conclusion

The previous note was directionally right, but underspecified. The important update is:

- ContextEngine is available locally now.
- The hook surface is wider than originally noted.
- MOON should not jump straight to replacing compaction.
- The best first step is a small assemble-first engine that keeps `legacy` compaction behavior via delegation.

## Source Notes

Local files inspected:

- `/Users/lilac/.nvm/versions/node/v24.13.0/lib/node_modules/openclaw/CHANGELOG.md`
- `/Users/lilac/.nvm/versions/node/v24.13.0/lib/node_modules/openclaw/docs/concepts/context.md`
- `/Users/lilac/.nvm/versions/node/v24.13.0/lib/node_modules/openclaw/docs/tools/plugin.md`
- `/Users/lilac/.nvm/versions/node/v24.13.0/lib/node_modules/openclaw/docs/plugins/manifest.md`
- `/Users/lilac/.nvm/versions/node/v24.13.0/lib/node_modules/openclaw/docs/gateway/configuration-reference.md`
- `/Users/lilac/.nvm/versions/node/v24.13.0/lib/node_modules/openclaw/dist/plugin-sdk/context-engine/types.d.ts`

Updated on 2026-03-08 after inspecting the local OpenClaw installation directly.
