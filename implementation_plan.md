# Moon System End-to-End Implementation Plan

This plan is an uninterrupted implementation sequence from **Phase 0 through Phase 8** for a standalone Moon System background service.

## Objective

Build a Rust-first background system that:
1. Monitors active OpenClaw session context usage continuously.
2. Triggers archive/index/prune/distill actions at defined thresholds.
3. Preserves original context in archives and QMD index before aggressive compression.
4. Creates a continuity handoff into a new distilled session.
5. Supports recall so archived context can be found and loaded back into active work.

## End-to-End Runtime Flow (Target Behavior)

1. `moon-watch` runs in daemon mode.
2. Watcher polls session usage and computes usage ratio.
3. At 80% usage: snapshot raw session and register archive in QMD.
4. At 85% usage: enforce aggressive pruning policy and run compaction flow.
5. At 90% usage: run semantic distillation, update daily memory, log audit output.
6. Distillation completion triggers continuity protocol:
- create new session
- inject semantic map as first context block
- link to archive + daily memory entries
7. When user asks about old context, `moon-recall` searches QMD and returns loadable context pack.

## Constraints

1. No API keys in repository.
2. BYO API keys only (env vars or external secret manager).
3. Archive and index must complete before destructive pruning/distillation actions.
4. All actions must be idempotent and auditable.
5. System must degrade gracefully if Gemini is unavailable.

## System Components

1. **Watcher**: background loop and threshold state machine.
2. **Session Usage Provider**: reads active-session usage metrics.
3. **Archive Pipeline**: raw snapshot writer + ledger.
4. **QMD Integration**: collection add and search wrappers.
5. **Prune Engine**: existing `oc-token-optim` compaction path.
6. **Distiller**: local fallback + Gemini 2.5 Flash Lite provider.
7. **Continuity Protocol**: session rollover and semantic map injection.
8. **Recall Engine**: QMD search and context rehydration.
9. **State Store**: persistent watcher state and cooldown markers.
10. **Audit/Observability**: structured logs and traceable action history.

## Repo Changes by Area

1. `src/moon/`:
- `watcher.rs`
- `thresholds.rs`
- `state.rs`
- `archive.rs`
- `distill.rs`
- `continuity.rs`
- `recall.rs`
- `session_usage.rs`
- `audit.rs`
- `config.rs`
2. `src/commands/`:
- `moon_watch.rs`
- `moon_recall.rs`
- `moon_distill.rs` (manual trigger/debug)
3. `src/cli.rs`:
- add new command surfaces.
4. `tests/`:
- watcher, threshold, distill, continuity, recall, failure-path integration tests.

## Phase-by-Phase Delivery

## Phase 0: Spec Lock and Contracts

### Goal
Define all external and internal contracts so implementation does not drift.

### Tasks
1. Define `moon.toml` schema with defaults:
- polling interval
- thresholds (0.80 / 0.85 / 0.90)
- cooldown windows
- provider config
- paths
2. Define data contracts:
- `SessionUsageSnapshot`
- `ArchiveRecord`
- `DistillationRecord`
- `ContinuityMap`
- `RecallResult`
3. Define state file schema at `~/.lilac_metaflora/state/moon_state.json`.
4. Define failure policy matrix (retry, skip, degrade, hard fail).

### Deliverables
1. `docs/contracts.md`
2. `docs/failure_policy.md`
3. `moon.toml.example`

### Exit Criteria
1. Schema validation tests pass.
2. Contract structs compile and serialize/deserialize.

## Phase 1: Runtime Foundation

### Goal
Create watcher runtime skeleton and command entrypoints.

### Tasks
1. Implement config loader with env override support.
2. Implement state persistence and file locking to avoid dual daemons.
3. Add CLI commands:
- `moon-watch --once`
- `moon-watch --daemon`
4. Add structured `CommandReport` outputs for watch operations.

### Deliverables
1. `src/moon/config.rs`
2. `src/moon/state.rs`
3. `src/commands/moon_watch.rs`

### Exit Criteria
1. `moon-watch --once` completes cleanly without side effects.
2. `moon-watch --daemon` loop runs and writes heartbeat.

## Phase 2: Session Usage Monitoring

### Goal
Get reliable live usage signals for threshold decisions.

### Tasks
1. Define `SessionUsageProvider` trait.
2. Implement provider A: OpenClaw API/CLI metrics.
3. Implement provider B fallback: latest session-file token estimator.
4. Normalize provider output to ratio + absolute tokens.
5. Add stale-data guardrails (timestamp freshness checks).

### Deliverables
1. `src/moon/session_usage.rs`
2. Provider selection in config.

### Exit Criteria
1. Watcher obtains usage snapshot in normal environment.
2. Fallback provider works when primary is unavailable.

## Phase 3: Archive + QMD Pipeline

### Goal
Guarantee zero-loss capture before context reduction.

### Tasks
1. Reuse snapshot logic to write raw session archive with deterministic naming.
2. Add content hash to dedupe repeated snapshots.
3. Add archive ledger file with metadata:
- source session id
- archive path
- hash
- timestamp
4. Execute QMD collection registration/index step.
5. Add retry policy around QMD operations.

### Deliverables
1. `src/moon/archive.rs`
2. Ledger at `~/.lilac_metaflora/archives/ledger.jsonl`

### Exit Criteria
1. Snapshot + QMD index run atomically in one pipeline call.
2. Re-run on same source does not duplicate ledger entries.

## Phase 4: Threshold State Machine

### Goal
Implement deterministic orchestration at 80/85/90 thresholds.

### Tasks
1. Implement finite states:
- `Normal`
- `ArchiveTriggered`
- `PruneTriggered`
- `DistillTriggered`
- `Cooldown`
2. Add hysteresis and cooldown timers.
3. Trigger rules:
- 80%: archive + index
- 85%: prune path
- 90%: distill path
4. Persist last-trigger metadata in state file.

### Deliverables
1. `src/moon/thresholds.rs`
2. State machine tests for edge transitions.

### Exit Criteria
1. No repeated trigger storms under fluctuating usage.
2. Ordered trigger sequence is preserved.

## Phase 5: Distillation Engine (Gemini + Local Fallback)

### Goal
Produce semantic compression from archived context while preserving signal.

### Tasks
1. Define `Distiller` trait.
2. Implement `LocalDistiller` baseline extraction (rules, decisions, milestones).
3. Implement `GeminiDistiller` using `gemini-2.5-flash-lite` when key/config present.
4. Output targets:
- append to `~/.lilac_metaflora/memory/YYYY-MM-DD.md`
- append audit event to `~/.lilac_metaflora/skills/moon-system/logs/audit.log`
5. Add timeout, retries, and fallback cascade (Gemini -> Local).

### Deliverables
1. `src/moon/distill.rs`
2. Distillation prompt templates and parser.

### Exit Criteria
1. Distillation always emits a valid structured record.
2. Missing/invalid Gemini key does not break pipeline.

## Phase 6: Continuity Protocol and Session Rollover

### Goal
Start a new concise working session without losing retrievability.

### Tasks
1. Build continuity map artifact containing:
- summary bullets
- archive file references
- daily memory references
- recall hints
2. Implement session rollover integration with OpenClaw session management.
3. Inject continuity map as first system context block into new session.
4. Link old and new session ids in ledger for traceability.

### Deliverables
1. `src/moon/continuity.rs`
2. Continuity map schema and serializer.

### Exit Criteria
1. New session boots with continuity context.
2. Old session can be reconstructed via ledger and archive references.

## Phase 7: Recall and Rehydration

### Goal
Allow agent/user to recover archived context quickly on demand.

### Tasks
1. Add `moon-recall --query "..." --json` command.
2. Execute `qmd search` against history collection.
3. Rank matches and construct rehydration pack:
- snippet
- source archive path
- relevance metadata
4. Optional: inject selected recall result into active session.

### Deliverables
1. `src/commands/moon_recall.rs`
2. `src/moon/recall.rs`

### Exit Criteria
1. Recall finds relevant archived material for known historical queries.
2. Rehydration payload is machine-readable and safe to inject.

## Phase 8: Production Hardening and Background Deployment

### Goal
Ship resilient unattended operation.

### Tasks
1. Add integration tests covering full watcher cycle.
2. Add chaos/failure tests:
- OpenClaw unavailable
- QMD unavailable
- distiller timeout
- state file corruption recovery
3. Add observability:
- structured logs
- counters and last-action markers
4. Add service packaging:
- `launchd` template for macOS
- `systemd` unit for Linux
5. Final docs:
- operations runbook
- troubleshooting
- security checklist

### Deliverables
1. `docs/runbook.md`
2. `deploy/launchd/*.plist`
3. `deploy/systemd/*.service`

### Exit Criteria
1. 24h daemon soak test passes.
2. Recovery from transient failures verified.
3. End-to-end scenario from 80% to recall works without manual intervention.

## End-to-End Definition of Done

1. One continuous run can execute:
- detect threshold
- archive raw session
- index with QMD
- prune/distill
- rollover to new session
- recall archived data later
2. Every stage emits auditable records with timestamps and IDs.
3. No repository-managed secrets.
4. Full CI test suite passes.
5. Documentation enables external users to install and run with BYO OpenClaw and BYO API keys.

## Execution Policy: No Stop Between Phases

1. After each phase exit criteria pass, immediately start the next phase in the same implementation stream.
2. Do not pause for redesign unless a hard blocker violates constraints or security policy.
3. Maintain backward compatibility for existing commands (`install`, `verify`, `post-upgrade`, `moon-snapshot`, `moon-index`, `moon-status`) while adding new Moon watcher capabilities.

## Recommended Work Sequence (Single Continuous Run)

1. Phase 0 -> Phase 1 -> Phase 2 -> Phase 3 -> Phase 4 -> Phase 5 -> Phase 6 -> Phase 7 -> Phase 8
2. At each phase boundary:
- run `cargo fmt --all`
- run `cargo clippy -- -D warnings`
- run `cargo test`
3. Keep a rolling `CHANGELOG` entry for each phase completion.

