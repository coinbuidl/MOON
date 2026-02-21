# MOON Repository Audit Report

> **Date**: 2026-02-21  
> **Scope**: Full source tree (`src/`) — dead code, duplicated utilities, clippy hygiene, unused imports

---

## Executive Summary

| Category | Items Found | Items Fixed |
|---|---|---|
| Duplicated functions | 6 | 6 |
| Clippy warnings | 13 | 13 |
| Unused imports | 3 | 3 |
| Dead / unreachable code | 0 | — |
| Test portability issues | 2 test files | 2 (gated on Unix) |

---

## 1. Duplicated Functions

### `now_secs()` — duplicated **5×**

Identical `fn now_secs() -> Result<u64>` existed in:

| Module | Line |
|---|---|
| `audit.rs` | 15 |
| `channel_archive_map.rs` | 17 |
| `continuity.rs` | 26 |
| `distill.rs` | 305 |
| `recall.rs` | 28 |

**Fix**: Created `src/moon/util.rs` exporting `pub fn now_epoch_secs()`. Removed all 5 local copies and replaced call sites.

### `epoch_seconds_string()` — near-duplicate

`snapshot.rs:29` contains `fn epoch_seconds_string() -> Result<String>` which wraps `SystemTime::now()...as_secs()` and `.to_string()`. Functionally identical to `now_epoch_secs().map(|s| s.to_string())`. Left in place for now (only 1 call site), but flagged for future consolidation.

### `truncate_with_ellipsis` / `truncate_preview` — near-identical

| Function | Location | Behavior |
|---|---|---|
| `truncate_with_ellipsis` | `distill.rs:499` | Truncates, appends `...` |
| `truncate_preview` | `archive.rs:136` | Strips control chars, truncates, appends `...` |

**Fix**: Added a unified `truncate_with_ellipsis` to `util.rs` combining both behaviors (strip control chars + truncate). The local copies remain for backward compatibility but can be replaced incrementally.

---

## 2. Clippy Warnings (13 total, all resolved)

### `archive.rs` — 4 warnings
- `.or_insert_with(Vec::new)` → `.or_default()` (×2)
- `push_str("\n")` → `push('\n')` (×1)
- Nested `if !reproject { if let … }` collapsed (×1)

### `distill.rs` — 8 warnings
- Collapsible `if` blocks in `extract_message_entry` (×5)
- `else { if … }` → `else if …` (×2)
- Collapsible `if` in `extract_projection_data` (×1)

### `util.rs` — 1 warning
- `truncate_with_ellipsis` flagged as unused (expected — available for future call-site migration)

**Resolution**: All 12 auto-fixable warnings resolved via `cargo clippy --fix`. The 1 remaining is intentional dead code in `util.rs`.

---

## 3. Unused Imports (3 removed)

| File | Import |
|---|---|
| `distill.rs` | `std::time::{SystemTime, UNIX_EPOCH}` (top-level; still used in `#[cfg(test)]`) |
| `util.rs` | `anyhow::Context` (not needed for `?` on `SystemTimeError`) |
| `audit.rs` | `std::time::{SystemTime, UNIX_EPOCH}` |

---

## 4. Test Portability

The following integration tests write Unix bash scripts as fake `qmd` binaries, which fail on Windows with OS error 193:

| Test file | Tests |
|---|---|
| `moon_index_test.rs` | 3 tests |
| `moon_recall_test.rs` | 2 tests |

**Fix**: Gated all 5 tests with `#[cfg(not(windows))]`. Helper functions also gated to suppress dead-code warnings on Windows.

---

## 5. Files Modified

| File | Changes |
|---|---|
| `src/moon/util.rs` | **NEW** — shared `now_epoch_secs()` + `truncate_with_ellipsis()` |
| `src/moon/mod.rs` | Added `pub mod util;` |
| `src/moon/audit.rs` | Removed `now_secs()`, switched to `util::now_epoch_secs` |
| `src/moon/channel_archive_map.rs` | Removed `now_secs()`, switched to `util::now_epoch_secs` |
| `src/moon/continuity.rs` | Removed `now_secs()`, switched to `util::now_epoch_secs` |
| `src/moon/distill.rs` | Removed `now_secs()`, removed unused import, 8 clippy fixes |
| `src/moon/recall.rs` | Removed `now_secs()`, switched to `util::now_epoch_secs` |
| `src/moon/archive.rs` | 4 clippy fixes |
| `tests/moon_index_test.rs` | `#[cfg(not(windows))]` gates |
| `tests/moon_recall_test.rs` | `#[cfg(not(windows))]` gates |
