# MIP-CONTEXT-CACHE: Gemini Context Caching Middleware Plan

## 1. Objective
Implement an automatic, transparent middleware layer within OpenClaw to leverage the Gemini `CachedContent` API. The goal is to drastically reduce per-token input costs and latency for sessions involving massive static payloads (e.g., long PDFs, extensive codebase contexts, or heavy system instructions) and prolonged chat histories.

## 2. Core Architectural Logic
The middleware acts as a proxy interceptor between OpenClaw's context compiler and the Gemini API, operating strictly on the structural array of the context window. It executes in a two-stage pipeline: **Sanitization** (to maximize signal density) followed by **Caching**.

### 2.1 Stage 1: Pre-Cache Sanitization (The Filter)
To avoid paying storage fees for noise and degrading the model's attention, the context array is scrubbed *before* caching. Specifically targeting `.jsonl` dumps and archive retrievals.

**Data to Keep (High-Signal):**
- **Core Semantic Content:** The raw text of the user query and the assistant's reply.
- **Identity Tags:** Broad role markers (`user`, `model`, `system`).
- **Critical Metadata:** Essential timestamps for chronological ordering (if specifically required for context) and absolute file paths (for code context).

**Data to Strip (Noise):**
- **JSON Overhead:** All structural brackets, commas, and formatting inherent to raw `.jsonl`.
- **System Identifiers:** Internal database IDs, internal message IDs, guild/channel IDs (unless routing requires them), and transaction hashes not requested by the user.
- **Redundant State Data:** Unnecessary `sender_id`, `conversation_label`, or repetitive untrusted metadata blocks attached to every single message in a dump.

### 2.2 Stage 2: The "Frozen Prefix" Model
Gemini caching requires the cached data to be the **prefix** of the context array. The middleware will identify and freeze this sanitized prefix using three logic gates:
1. **Intrinsic Static Data**: All `system` role instructions and file attachments (images, PDFs, codebase dumps) located at the start of the array.
2. **History Cursor**: A sliding window that freezes older conversation turns (e.g., locking turns 1 through `N-10`, leaving the 10 most recent turns dynamic).
3. **Deterministic Hashing**: The frozen prefix array is serialized and cryptographically hashed (SHA-256). This hash acts as the state identifier.

### 2.2 Execution Flow
1. **Intercept**: Middleware receives the full `messages` array intended for `generateContent`.
2. **Evaluate & Hash**: 
   - Extract the intended static prefix (System + Files + History up to Cursor).
   - Calculate the SHA-256 hash of this prefix.
3. **Cache Resolution**:
   - *Cache Miss (or Hash Mismatch)*: Send the prefix to the `CachedContent` API. Store the returned `Cache ID` mapping it to the hash. (If a previous cache existed for this session, issue a delete command to avoid orphaned storage costs).
   - *Cache Hit (Hash Match)*: Retrieve the existing `Cache ID`.
4. **Payload Rewrite**: 
   - Strip the frozen prefix from the outgoing `generateContent` request.
   - Insert the `Cache ID` into the request payload.
   - Append only the remaining *dynamic* messages (the delta) to the request.
5. **Send**: Transmit the optimized payload to the Gemini API.

## 3. Lifecycle & Cost Management
Caches incur hourly storage fees. To prevent runaway costs, the middleware must actively manage cache lifecycles:
- **Default TTL**: Set a strict Time-To-Live (e.g., 1 hour) upon creation.
- **TTL Extension**: If a cache is hit, optionally patch the TTL to extend it if the session is highly active.
- **Aggressive Eviction**: On session termination, or when the frozen prefix hash changes (cursor moves), immediately send a `DELETE` request for the old `Cache ID`.

## 4. Dual-Path Retention Design (Reasoning vs Retrace)
Adopt a dual-path strategy:
1. **Reasoning Path (to API):** Send only sanitized, high-signal context.
2. **Retrace Path (local record):** Use `moon/raw/*.jsonl` as the retrace source-of-truth for operational forensics.

### 4.1 Retrace Source Scope (7-Day Retention)
- **Primary Format:** Raw JSONL copies in `moon/raw/*.jsonl`.
- **Retention:** 7 days, then automatic deletion.
- **Purpose:** Post-incident traceability, debugging, provenance checks, and replay.
- **Optional Convenience Layer:** Generate lightweight `.md` summaries only for human review (non-authoritative).

### 4.2 Fields to Preserve in Retrace Source
- Message chronology (ISO timestamps).
- Role + speaker labels.
- Source surface/channel context (coarse level).
- Stable message references (message IDs) for replay/debug.
- File/source references (paths, archive pointers such as `archive_jsonl_path`).
- Filter/caching decisions (e.g., prefix hash, cache ID used, hit/miss, TTL).

### 4.3 Privacy & Safety Controls
- Keep retrace files local to workspace storage.
- Redact sensitive payloads where possible (secrets/tokens).
- Restrict access to operational/debug contexts only.
- Enforce automatic purge at 7 days with no manual dependency.

## 5. Implementation Milestones
- [ ] **Phase 1: Structural Diffing & Hashing Engine**
  - Implement the logic to split the message array into `Prefix` and `Delta`.
  - Implement deterministic SHA-256 hashing for the `Prefix` array.
- [ ] **Phase 2: Sanitization + Dual-Path Writer**
  - Build pre-cache sanitization for `.jsonl`-heavy context.
  - Use `moon/raw/*.jsonl` as retrace source-of-truth with 7-day TTL.
  - Optionally generate non-authoritative `.md` summaries for human reading.
- [ ] **Phase 3: API Integration**
  - Add standard CRUD operations for the Gemini `CachedContent` endpoint.
  - Modify the primary API dispatcher to conditionally accept a `Cache ID` and rewrite the payload.
- [ ] **Phase 4: Lifecycle Manager (The Sweeper)**
  - Implement TTL tracking and automated cleanup for both cache objects and `moon/raw/*.jsonl` retrace records.
  - Ensure zero orphaned caches and strict 7-day retrace purging.
- [ ] **Phase 5: OpenClaw Integration**
  - Expose configuration flags (e.g., `gemini.auto_cache_threshold_tokens`, `gemini.retrace_retention_days`).
