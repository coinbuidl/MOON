# Moon JSONL Retention Upgrade Implementation Plan

## Scope
- Repo: `moon-system` only.
- Language: Rust for runtime changes.
- External dependency boundary: QMD is external and read-only from Moon. Moon may call QMD CLI, but must not modify QMD codebase.

## Execution Mode
- Phase-by-phase execution with no approval checkpoints between phases.
- Rule: if a phase passes its gate checks, continue immediately to the next phase.
- Rule: if a phase fails, fix within the same phase and re-run gates before advancing.

## Target Outcomes
- Canonical distill trigger: idle mode with short idle window (`idle_secs=360`).
- Preserve current low footprint setup: clone repo, set `.env`, run.
- AI-readable warnings for all critical partial failures.
- Architecture remains compatible with a future `archive_event` trigger mode, but does not implement it now.

## Current Behavior Baseline (to preserve where valid)
- Distill candidate selection already prioritizes oldest pending archives by day and respects `max_per_cycle`.
- QMD indexing/search integration already exists through CLI calls.
- Archive snapshots are written as new timestamped files (dedup prevents duplicate ledger records for same source/content).

## Phase 1: Canonical Trigger and Config Baseline
- Goal: remove trigger ambiguity and standardize on practical idle distillation.
- Changes:
1. Set default idle distill window to 6 minutes (`distill.idle_secs = 360`) in config defaults.
2. Update `moon.toml.example` to match 6-minute default and keep `mode = "idle"`.
3. Keep `max_per_cycle` as a testing control parameter; no unlimited sentinel value in this phase.
4. Update docs to state exact runtime behavior:
   `distill starts after latest archive is idle for 360s and processes oldest pending day first`.
- Files:
1. `src/moon/config.rs`
2. `moon.toml.example`
3. `README.md`
4. `docs/runbook.md`
- Gate checks:
1. `cargo test`
2. `cargo run -- moon-watch --once` (with test env) reports `distill.idle_secs=360`.
3. Docs contain one consistent trigger definition (no 24h wording, no archival-trigger wording).

## Phase 2: Distill Ordering and Throughput Validation
- Goal: validate that testing with small `max_per_cycle` is deterministic and safe before scaling up throughput.
- Changes:
1. Add/extend tests proving ordering:
   oldest `created_at_epoch_secs` day selected first, then up to `max_per_cycle`.
2. Add test for mixed ledgers (indexed/unindexed/missing files/already distilled) to confirm only eligible records are selected.
3. Document throughput tuning path:
   start low (`max_per_cycle=1`), increase after stable cycles.
- Files:
1. `tests/moon_watch_test.rs`
2. `README.md`
3. `docs/failure_policy.md`
- Gate checks:
1. New tests pass and are deterministic.
2. One-shot watcher logs show expected candidate selection notes.

## Phase 3: AI-Readable Warning Contract
- Goal: warnings are explicit, machine-parseable, and indicate failing stage + skipped action.
- Warning format (single line):
`MOON_WARN code=<CODE> stage=<STAGE> action=<ACTION> session=<SESSION_ID> archive=<ARCHIVE_PATH> source=<SOURCE_PATH> retry=<RETRY_POLICY> reason=<REASON> err=<ERR_SUMMARY>`
- Required warning codes:
1. `INDEX_FAILED`
2. `DISTILL_FAILED`
3. `DISTILL_CHUNKED_FAILED`
4. `CONTINUITY_FAILED`
5. `RETENTION_DELETE_FAILED`
6. `LEDGER_READ_FAILED`
- Changes:
1. Add a small warning helper in Moon runtime layer to normalize format.
2. Emit warnings where the watcher currently only degrades silently or logs unstructured detail.
3. Keep existing audit events; warnings complement audit, not replace it.
- Files:
1. `src/moon/watcher.rs`
2. `src/moon/archive.rs`
3. `src/moon/audit.rs` (if helper location is better there)
4. `docs/failure_policy.md`
- Gate checks:
1. Failure-path tests assert warning line contains `code=`, `stage=`, and `action=`.
2. No panic/regression in normal successful cycles.

## Phase 4: Retention and Prune Safety Hardening
- Goal: implement clear retention behavior that is safe and aligned with distilled state.
- Changes:
1. Define and implement retention windows in Moon config/docs (active/warm/cold).
2. Ensure destructive cleanup only runs when distillation record exists for archive.
3. Ensure prune/delete flow updates all related state atomically enough for retry safety:
   ledger, channel map, distilled state markers, and QMD update call.
4. Keep behavior minimal-footprint: local file operations + existing QMD CLI calls only.
- Files:
1. `src/moon/watcher.rs`
2. `src/moon/archive.rs`
3. `src/moon/channel_archive_map.rs`
4. `src/moon/state.rs`
5. `docs/failure_policy.md`
6. `README.md`
- Gate checks:
1. Tests for age-boundary behavior.
2. Tests for failure rollback/retry semantics (no orphan references after partial failure).

## Phase 5: Future-Safe Trigger Extension Point (No Feature Delivery Yet)
- Goal: avoid design clash with future `archive_event` mode while shipping idle mode now.
- Changes:
1. Introduce internal trigger abstraction enum/interface in watcher selection path.
2. Keep exposed config values unchanged for this phase (`manual` and `idle`), but structure code so `archive_event` can be added without refactor churn.
3. Add one non-executed placeholder test/module comment documenting expected archive-event semantics.
- Files:
1. `src/moon/watcher.rs`
2. `src/moon/config.rs`
3. `docs/contracts.md`
- Gate checks:
1. Existing modes behave exactly as before.
2. No dead code warnings and no user-facing behavior change in this phase.

## Phase 6: Minimal-Footprint Packaging and Operator UX
- Goal: make onboarding "clone + `.env` + run" reliable for new users.
- Changes:
1. Tighten `.env.example` to include only must-have and strongly recommended fields first.
2. Add a short bootstrap command sequence focused on one successful cycle.
3. Ensure warning docs include quick triage table for AI agent operators.
- Files:
1. `.env.example`
2. `README.md`
3. `docs/runbook.md`
- Gate checks:
1. Fresh-machine simulation works with documented minimum setup.
2. `cargo run -- verify --strict` and one-shot watch pass with documented env.

## Rollout Sequence
1. Implement phases in order from 1 to 6.
2. Keep each phase in a separate commit for rollback clarity.
3. After each phase, run `cargo test` before continuing.

## Non-Goals (This Upgrade)
- No changes to QMD repository internals.
- No implementation of archive-event trigger mode in this cycle.
- No heavy new dependencies that increase runtime footprint.

## Acceptance Criteria
1. Idle distillation defaults to 6 minutes and is documented consistently.
2. Distill ordering is deterministic and test-covered.
3. Warning lines are AI-readable and emitted on all key failure paths.
4. Retention behavior is explicit, safe, and test-covered.
5. New users can run Moon with minimal `.env` setup and no QMD repo modifications.
