# UBL Runtime Blueprint (Senior)

**Goal**: One invocable TDLN used by every gate/runtime, with deterministic ρ/PURE canon, D8 binding, and clean sidecar.

## Components
- **`ubl_tdln`**: library with:
  - `canon`: canonical JSON (sorted keys, NFC strings, null-stripping, i64-only).
  - `cid`: BLAKE3-based `b3:<hex64>`.
  - `bind` (D8): name-match, fallback 1↔1, else error.
  - `engine::execute(manifest, vars, cfg)`: `parse → policy → render`, returns `{artifacts, cid, dimension_stack}`.
- **`ubl_tdln_svc`**: Axum sidecar exposing `POST /v1/execute`.

## Determinism gates
- No time/clock, no randomness, pure functions.
- Canonicalization before hashing.
- Tests: determinism 10x; binding tests; canon equivalence.

## How to evolve
- Add `policy` dialects behind feature flags.
- Extend `codec` table (e.g., `json.path`, `base64.encode`, `bytes.len`).
- Optional WASM plugin boundary for third-party grammars.

## Ops
- `make build|test|run|conformance`.
- Sidecar on :3300.
- Logs: compact INFO.
