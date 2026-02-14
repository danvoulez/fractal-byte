# UBL Battle++ Report — Strict Audit

**Date:** 2026-02-14  
**Branch:** `main` @ `8906bd1`  
**PRs Merged:** 12/12 (D1–D5 docs + I1–I8 features)  
**Session delta:** 101 files changed, +12,117 / −611 lines  
**Rust:** ~8,500 lines | **Total project:** ~15,500 lines | **Tests:** 180 green

---

## PART A — CRITICAL BUGS (must fix before any release)

### BUG-1: CORS preflight blocked by auth middleware [SEVERITY: HIGH]

**File:** `services/ubl_gate/src/lib.rs:286-348`

Axum applies layers in **reverse order**. The auth middleware (line 343) runs
**before** the CORS layer (line 305). This means:

- Browser sends `OPTIONS` preflight with `Origin:` header
- Auth middleware rejects it with 401 (no Bearer token on preflight)
- CORS layer never runs → browser gets a CORS error on every cross-origin request

**Impact:** CORS is completely broken in production when `UBL_AUTH_DISABLED=0`.
Every browser-based client (UI, SDKs from browser) will fail.

**Fix:** Either skip auth for OPTIONS requests, or reorder layers so CORS runs
before auth.

### BUG-2: `list_receipts` and `audit_report` have no tenant isolation [SEVERITY: HIGH]

**File:** `services/ubl_gate/src/api.rs:274-291`

```rust
pub async fn list_receipts(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.receipt_chain.read().unwrap();
    (StatusCode::OK, Json(json!(*store)))  // dumps ALL tenants' receipts
}

pub async fn audit_report(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.receipt_chain.read().unwrap();
    // ... iterates ALL receipts regardless of tenant
}
```

Both endpoints dump the **entire** receipt chain across all tenants. Any
authenticated client can see every other tenant's receipts and audit data.

**Impact:** Complete tenant data leak. Multi-tenancy is broken for reads.

### BUG-3: `execute_runtime` stores receipts without tenant scoping [SEVERITY: HIGH]

**File:** `services/ubl_gate/src/api.rs:354-367`

Receipts are stored in a flat `HashMap<String, Value>` keyed by `body_cid`.
There is no tenant prefix or namespace. This means:

- Tenant A's receipts are visible to Tenant B via `/v1/receipts`
- CID collisions across tenants would overwrite each other
- Audit reports mix all tenants' data

### BUG-4: `chrono_now_iso()` produces wrong dates [SEVERITY: MEDIUM]

**File:** `crates/ubl_runtime/src/receipt.rs:56-80`

```rust
let y = 1970 + days / 365;  // ignores leap years
let mo = doy / 30 + 1;      // months aren't 30 days
let day = doy % 30 + 1;     // can produce day=31 for Feb
```

This hand-rolled date function drifts ~1 day per 4 years from epoch. On
2026-02-14, it produces approximately `2026-02-15` or similar. The comment says
"good enough for observability" but these timestamps appear in receipts that
may be used for audit/legal purposes.

**Impact:** Incorrect timestamps in receipt `observability.logline.when_iso`.
Not a security issue but undermines audit trail credibility.

---

## PART B — SECURITY GAPS (must address before production)

### SEC-1: Dev signing key is deterministic and hardcoded [SEVERITY: HIGH]

**File:** `crates/ubl_runtime/src/receipt.rs:122-130`

```rust
pub fn dev() -> Self {
    Self {
        active: ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]),
        active_kid: "did:dev#k1".into(),
```

The `KeyRing::dev()` uses `[7u8; 32]` as the signing key. This is used in
`AppState::default()`. There is **no mechanism** to load real keys from env/file
in production. Anyone who reads the source can forge valid JWS signatures for
any receipt.

**Fix needed:** Key loading from env var or file, with `dev()` only used when
`UBL_AUTH_DISABLED=1`.

### SEC-2: Token store is in-memory only, no persistence [SEVERITY: MEDIUM]

**File:** `services/ubl_gate/src/lib.rs:116-148`

`TokenStore` is a `HashMap` in memory. On restart, all registered tokens are
lost except the hardcoded dev token. `config/seed_tokens.json` exists but is
**never loaded** by the gate — it's documentation only.

**Impact:** No way to manage tokens in production. Every restart resets auth.

### SEC-3: `auth_disabled` defaults to `true` [SEVERITY: MEDIUM]

**File:** `services/ubl_gate/src/lib.rs:257-259`

```rust
let auth_disabled = std::env::var("UBL_AUTH_DISABLED")
    .map(|v| v == "1")
    .unwrap_or(true);  // DEFAULT IS TRUE = AUTH OFF
```

If `UBL_AUTH_DISABLED` is not set, auth is **disabled**. This is backwards —
production should default to auth enabled, with an explicit opt-out for dev.

### SEC-4: Rate limiter buckets grow unbounded [SEVERITY: LOW]

**File:** `services/ubl_gate/src/lib.rs:44`

```rust
buckets: Arc<Mutex<HashMap<String, Bucket>>>,
```

No eviction. Every unique `client_id` creates a permanent bucket entry. An
attacker can exhaust memory by sending requests with unique `X-Client-Id`
headers (when auth is disabled, `client_id` falls back to "anonymous", but
with auth enabled each client gets a bucket).

### SEC-5: Policy condition evaluator fails open [SEVERITY: MEDIUM]

**File:** `crates/ubl_runtime/src/policy.rs:219,244`

```rust
return true; // unparseable → pass (fail-open for unknown conditions)
// ...
true // Unknown condition → pass (fail-open)
```

Any unrecognized condition expression silently passes. A typo in a policy rule
(e.g., `"inpust.brand_id"` instead of `"inputs.brand_id"`) will never deny.

---

## PART C — ARCHITECTURAL GAPS

### GAP-1: All state is in-memory — zero persistence

`receipt_chain`, `transition_receipts`, `seen_cids`, `last_tip`, `token_store`
are all `Arc<RwLock<HashMap>>`. On process restart:

- All receipts are lost
- All transition receipts are lost
- Idempotency tracking resets (duplicate requests will succeed)
- Chain tip resets (chaining breaks)

`ubl_ledger` has an S3 stub but it's feature-gated and **not wired** into the
gate's receipt storage path.

### GAP-2: `execute_runtime` has TOCTOU race on idempotency

**File:** `services/ubl_gate/src/api.rs:338-387`

The handler reads `seen_cids` snapshot, passes it to `run_with_receipts`, then
writes the new key back. Between the read and write, another concurrent request
with the same inputs can pass the idempotency check. The `race_card` test
exists but only tests 1 concurrent request.

### GAP-3: `get_receipt` doesn't use the receipt_chain store

**File:** `services/ubl_gate/src/api.rs:163-177`

`get_receipt` calls `ubl_receipt::get_receipt(&cid)` which uses the
`ubl_receipt` crate's own store — **not** the gate's `receipt_chain` HashMap.
So receipts stored by `execute_runtime` are invisible to `GET /v1/receipt/:cid`.
Only `GET /v1/receipts` (list_receipts) shows them.

### GAP-4: CORS predicate always passes `tenant_id: None`

**File:** `services/ubl_gate/src/lib.rs:308-313`

```rust
move |origin: &HeaderValue, _parts: &axum::http::request::Parts| {
    origin.to_str()
        .map(|o| cors_config.is_origin_allowed(o, None))  // always None!
        .unwrap_or(false)
}
```

Per-tenant CORS origins are configured but **never used**. The predicate has no
access to the authenticated tenant_id (auth runs after CORS in Axum's layer
order). Only global origins are ever checked.

### GAP-5: Adapter HTTP is not wired into the gate

The `ubl_adapter` crate exists with policy checking and CID pinning, but there
is **no gate endpoint** that invokes it. `POST /v1/execute` runs the runtime
engine, not the adapter. The adapter is a standalone library with no integration
point.

### GAP-6: CLI `ublx` uses `reqwest::blocking` — no async

**File:** `crates/ublx/src/commands.rs`

The CLI uses `reqwest::blocking::Client` which spawns a tokio runtime per
request. This works but is inefficient and will cause issues if the CLI ever
needs to do concurrent operations (e.g., batch verify).

### GAP-7: SDK smoke tests not wired into CI

The TS and Python SDKs have unit tests but no CI job runs them. The CI only
runs Rust tests. `npm test` and `pytest` are never executed in GitHub Actions.

### GAP-8: `execute` and `execute_with_cascade` are duplicated

**File:** `crates/ubl_runtime/src/engine.rs:86-218`

Two nearly identical 60-line functions that differ only in how they call the
policy resolver. Should be a single function with an optional cascade parameter.

### GAP-9: No request logging / tracing

The gate has Prometheus counters but no structured request logging. No request
IDs, no trace correlation, no error logging. The `metrics_middleware` records
counts and latency but individual request failures are silent.

### GAP-10: UI is not served by the gate

The `ui/` directory contains a React app but the gate has no static file
serving. `ui.ubl.agency` DNS exists but points to `:3001` which has no server.
The UI is dead code.

---

## PART D — TEST COVERAGE GAPS

### TESTGAP-1: No integration test for the full HTTP flow

The 180 unit tests test individual functions. The `hardened.rs` tests use
`axum::test` but don't test the actual middleware stack ordering (which is
where BUG-1 lives). No test sends a real CORS preflight through the full
middleware chain with auth enabled.

### TESTGAP-2: No multi-tenant isolation test

No test verifies that Tenant A cannot see Tenant B's receipts. The
`setup_multi_tenant` test helper exists in `hardened.rs` but only tests
auth — not data isolation.

### TESTGAP-3: No test for `get_receipt` returning stored receipts

`GET /v1/receipt/:cid` is tested against `ubl_receipt`'s store, but never
against receipts created by `POST /v1/execute`. This would have caught GAP-3.

### TESTGAP-4: No concurrent idempotency test

The `race_card` test runs 1 request. No test sends 100 concurrent identical
requests to verify the idempotency check under contention.

### TESTGAP-5: SDK tests are offline mocks only

Both TS and Python SDK test suites mock all HTTP calls. No test actually hits
a running gate. The battle script was supposed to fill this gap but has its
own bugs.

---

## PART E — WHAT WORKS WELL

Despite the above, the core architecture is sound:

- **Receipt-first pipeline** (WA → Transition → WF) with chained CIDs and JWS
  signatures is correctly implemented and deterministic
- **RB-VM** (27 law tests) is solid — fuel metering, TLV bytecode, all 19 opcodes
- **NRF-1.1 canonicalization** (32 tests) handles Unicode NFC, sorted keys, BOM stripping
- **JWS detached signatures** (RFC 7797) are correctly implemented with Ed25519
- **Policy cascade resolver** correctly implements global → tenant → app ordering
  with DENY short-circuit and WARN continuation
- **Edge protections** (413/415) work correctly
- **CI pipeline** is comprehensive for Rust (fmt + clippy + test + conformance)
- **Release pipeline** (tag → build → deploy) is fully automated

---

## PART F — PRIORITY FIX ORDER

| Priority | Item | Effort |
|----------|------|--------|
| **P0** | BUG-1: CORS preflight blocked by auth | 30 min |
| **P0** | BUG-2+3: Tenant isolation on reads/writes | 2 hrs |
| **P0** | SEC-1: Production key loading | 1 hr |
| **P0** | GAP-3: `get_receipt` uses wrong store | 30 min |
| **P1** | SEC-3: Default auth to enabled | 10 min |
| **P1** | SEC-5: Fail-closed on unknown policy conditions | 30 min |
| **P1** | SEC-2: Load seed_tokens.json on boot | 1 hr |
| **P1** | GAP-4: Per-tenant CORS actually works | 1 hr |
| **P2** | BUG-4: Use chrono for timestamps | 15 min |
| **P2** | GAP-1: Wire S3 ledger for persistence | 4 hrs |
| **P2** | GAP-2: Atomic idempotency check | 1 hr |
| **P2** | GAP-5: Wire adapter into gate | 2 hrs |
| **P3** | GAP-7: SDK tests in CI | 1 hr |
| **P3** | GAP-9: Structured request logging | 2 hrs |
| **P3** | GAP-10: Serve UI from gate or separate process | 1 hr |

---

## Summary

> **The codebase has strong foundations** — the receipt pipeline, VM, canonicalization,
> JWS, and policy engine are well-tested and correctly implemented.
>
> **But it is NOT production-ready.** There are 4 critical bugs (CORS broken with
> auth, no tenant isolation on reads, hardcoded signing keys, receipt store
> mismatch), 5 security gaps, and 10 architectural gaps that must be addressed.
>
> The 180 unit tests give false confidence because they don't test the middleware
> integration (where the real bugs live) or multi-tenant data isolation.
>
> **Estimated effort to reach production-ready:** ~15-20 hours of focused work
> on P0+P1 items.
