# moon plugin

`moon` compacts large `toolResult` text blocks at persist time using the `tool_result_persist` hook.

## Stage 2 behavior

1. Token-aware + char-aware budget enforcement.
2. Per-tool limits (global defaults with per-tool overrides).
3. JSON projection for high-volume tools (`read`, `message/readMessages`, `message/searchMessages`, `web_fetch`, `web.fetch`).
4. Metadata persisted to `details.ocTokenOptim` with before/after estimated tokens.
5. Optional full payload retention in `details.ocTokenOptim.fullText` when under `maxRetainedBytes`.

## Plugin config

Under `plugins.entries.moon.config`:

1. `maxTokens` (default `12000`)
2. `maxChars` (default `60000`)
3. `maxRetainedBytes` (default `250000`)
4. `tools.<tool>.maxTokens`
5. `tools.<tool>.maxChars`
