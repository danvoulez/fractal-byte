# UBL Laws

> Canonical invariants. These are not guidelines — they are laws.
> Violating any of these is a bug, not a tradeoff.

## Law 1: No-IO in the Runtime

The runtime (`crates/ubl_runtime`) performs **zero** IO operations.
No network, no filesystem, no clock, no randomness.

IO is only permitted via Wasm adapters, and the result **must** be frozen by CID before the runtime touches it.

```text
Rule of thumb:
  Same result tomorrow?     → Runtime handles it.
  Depends on world/time?    → Wasm acquires; runtime seals.
```

**Enforcement**: `crates/rb_vm` has no IO syscalls by construction. The `CanonProvider` trait is the only external interface, and it is deterministic (CAS reads + signature verification).

## Law 2: Determinism (Same Input → Same Output → Same CID)

Given identical inputs, the pipeline **must** produce identical bytes, identical CIDs, and identical receipts (excluding observability timestamps).

**Enforcement**: `cargo test -p ubl_runtime -- determinism` runs the same execution 10× and asserts CID equality.

## Law 3: Single Pipeline (rb → ρ → WA → exec → WF)

Every entry follows the same path. No shortcuts, no alternative routes.

```text
input → RB(-1) → ρ(0) → WA → execute → WF → receipt
```

Each passage generates or extends a receipt. The pipeline is fractal: the same structure applies whether processing a 10-byte string or a 10 MB document.

**Enforcement**: `run_with_receipts()` in `crates/ubl_runtime/src/receipt.rs` is the single entry point. It always produces `receipts.{wa, transition, wf}`.

## Law 4: Receipt is State

The receipt is the truth of the state — not a log, not a side effect.

- No separate RID. Identity comes from CID + parents→tip chain.
- The ledger is a **consequence** of the receipt, not the other way around.
- The receipt can never fail. The ledger can.
- URL is transport; verification is by content (CID).

**Enforcement**: every `/v1/execute` response includes `receipts` and `tip_cid`. The receipt is built before any ledger write.

## Law 5: Transition -1→0 is Mandatory

Every execution that touches RB-VM **must** produce a transition receipt proving the jump from layer -1 (preimage) to layer 0 (ρ/canonical form).

The transition contains:
- `preimage_raw_cid`: CID of the raw RB output
- `rho_cid`: CID after NRF canonicalization
- `witness`: `{vm_tag, bytecode_cid, fuel_spent}`

**Enforcement**: `build_transition()` in `crates/ubl_runtime/src/transition.rs`. Schema: `schemas/transition.schema.json`.

## Law 6: Chained Receipts (Immutable, Append-Only)

Receipts are chained by parent CIDs. Each receipt's `parents` field references the CIDs of its predecessors. The chain is:

```text
WA.cid → Transition.cid (parents: [WA.cid]) → WF.cid (parents: [Transition.cid])
```

- Receipts are immutable (CID-addressed). No updates, only appends.
- Replaying the same input with the same parents → 409 CONFLICT (idempotency).
- Tampering with any receipt breaks the chain (CID mismatch).

**Enforcement**: `services/ubl_gate/tests/hardened.rs` — replay integrity and DENY chain integrity tests.

## Law 7: Ghost by Default

WA and WF records are `ghost: true` by default — auditable but not published externally.
The **receipt** is public (unless policy/capsule restricts it).

Ghost records go to the audit trail and appear in the receipt, but are filtered from external telemetry.

**Enforcement**: `ghost` field in WA/WF JSON. See `docs/runtime-ghost-write-ahead.md`.

## Law 8: Canonical Form (NRF-1.1)

All data is canonicalized before hashing:

- Keys sorted by Unicode codepoint
- Strings in NFC normalization
- Null values stripped
- BOM removed
- Numbers as i64 when exact; decimals as canonical strings
- NaN/Infinity prohibited

**Enforcement**: `crates/ubl_ai_nrf1` implements NRF-1.1. Decision D1 in `docs/spec/RB_VM_DECISIONS.md`.

## Law 9: Verifiable Signatures (JWS EdDSA)

Every receipt is signed with JWS EdDSA (RFC 7797, `b64=false`).
The `kid` field points to a DID published at `/.well-known/did.json`.

- `sign_detached()` and `verify_detached()` in `crates/ubl_runtime/src/jws.rs`
- Key rotation via `KeyRing` in `services/ubl_gate/src/lib.rs`

**Enforcement**: `cargo test -p ubl_runtime -- jws` and receipt verification tests.

## Law 10: Mandatory Narrative on Critical Denies

When the pipeline produces a DENY decision, the WF **must** include a human-readable `reason` and `rule_id`.
The narrative is LLM-first: short, actionable, and auditable.

- DENY → WF with `decision: "DENY"`, `reason: "RULE_X_BLOCKED"`, `rule_id: "RULE_X"`
- RETRY → receipt with recommendation for next action
- The UI surfaces this as a prominent status (PASS / RETRY / DENY)

**Enforcement**: `services/ubl_gate/tests/hardened.rs` — DENY chain integrity tests verify narrative presence.

---

## Invariant Summary

| # | Law | Violated by |
|---|-----|-------------|
| 1 | No-IO in runtime | Any syscall, network call, or clock read in `ubl_runtime` |
| 2 | Determinism | Different CID for same input |
| 3 | Single pipeline | Bypassing any stage (rb, ρ, WA, exec, WF) |
| 4 | Receipt is state | Writing to ledger before receipt is built |
| 5 | Transition mandatory | Executing RB without transition proof |
| 6 | Chained receipts | Missing parent CIDs, accepting duplicate without 409 |
| 7 | Ghost by default | Publishing WA/WF externally without policy override |
| 8 | NRF-1.1 canon | Hashing non-canonical bytes |
| 9 | JWS EdDSA | Unsigned receipt, invalid kid |
| 10 | Narrative on deny | DENY without reason/rule_id |
