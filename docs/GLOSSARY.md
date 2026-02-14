# UBL Glossary

Canonical terminology for the UBL system. Use these terms consistently across code, docs, and UI.

## Core Concepts

- **CID** — Content Identifier. BLAKE3 hash of NRF-canonical bytes, prefixed `b3:<hex64>`. The universal address for any piece of data. Verification is always by content, never by URL.

- **DID** — Decentralized Identifier. Format: `did:cid:<b3:hash>` or `did:key:<pubkey>`. Used for runtime identity, object identity, and key resolution via `/.well-known/did.json`.

- **NRF** — Normal Representation Form (version 1.1). The canonical JSON encoding: sorted keys by codepoint, NFC strings, null-stripped, BOM-removed, i64-only numbers. Implemented in `crates/ubl_ai_nrf1`.

- **Receipt** — The materialization of an execution's state and trajectory. Contains: timestamp, parent CIDs, body, body_cid, JWS proof, and observability data. The receipt IS the state — not a log entry.

- **TIP** — The CID of the most recent receipt in a chain. Following parents→tip reconstructs the full execution history.

## Pipeline Stages

- **RB / RB-VM** — Reasoning Bytecode Virtual Machine (`crates/rb_vm`). Layer -1. Deterministic stack machine with 19 opcodes, TLV bytecode encoding, and fuel metering. Zero IO by construction.

- **ρ (rho)** — The canonical form at layer 0. Raw bytes from RB-VM are canonicalized via NRF-1.1 and assigned a CID. The ρ is the "frozen" representation that the runtime operates on.

- **WA (Write-Ahead)** — Record materialized **before** execution. Captures intent: inputs, chip reference, policy, environment. Ghost by default. Phase: `before`.

- **WF (Write-After)** — Record materialized **after** execution. Captures result: decision (ALLOW/DENY), output artifacts, rule_id, reason. Ghost by default. Phase: `after`.

- **Transition** — Receipt proving the jump from layer -1 (RB-VM preimage) to layer 0 (ρ canonical form). Contains `preimage_raw_cid`, `rho_cid`, and witness (`vm_tag`, `bytecode_cid`, `fuel_spent`).

## Execution Outcomes

- **PASS (ALLOW)** — Execution succeeded. WF contains `decision: "ALLOW"`, `output_cid`, and artifacts.

- **DENY** — Execution rejected by policy. WF contains `decision: "DENY"`, `reason`, `rule_id`. A narrative is mandatory (Law 10).

- **RETRY** — Transient failure. Receipt includes recommendation for next action. HTTP 429 with `Retry-After` header when rate-limited.

## Security & Auth

- **Bearer Token** — Authentication via `Authorization: Bearer <token>` header. Tokens map to `client_id` and `allowed_kids`.

- **KID** — Key Identifier. Scopes what a client can access. `ClientInfo.kid_allowed()` enforces scope; mismatch → 403 FORBIDDEN.

- **JWS** — JSON Web Signature (RFC 7797, `b64=false`, EdDSA/Ed25519). Used for receipt signing. `sign_detached()` / `verify_detached()` in `crates/ubl_runtime/src/jws.rs`.

- **KeyRing** — Key rotation mechanism in the gate. Manages signing keys for receipt proofs.

- **Ghost** — A record marked `ghost: true` is auditable but not published externally. WA and WF are ghost by default; the receipt is public.

## Infrastructure

- **Gate** — The HTTP service (`services/ubl_gate`) on `:3000`. Entry point for all API calls. The runtime is a library inside the gate, not a separate process.

- **Ledger** — Storage abstraction for receipts and records. In-memory by default; S3 driver feature-gated (`crates/ubl_ledger`).

- **Manifest** — JSON document describing an execution: pipeline name, input/output grammars, policy, and variable bindings. See `docs/example.manifest.json`.

- **Chip** — A reusable execution unit referenced by CID. Contains bytecode for the RB-VM.

- **Capsule** — Privacy wrapper. Sensitive data is wrapped with a proof/pointer in WA/WF/receipt; the content itself is not stored in the receipt.

## Multi-Tenancy

- **Tenant** — An isolated organizational unit. Each tenant has its own policies, tokens, and allowed origins.

- **Policy Cascade** — Policies resolve in order: UBL (global) → Tenant → App/Project. A lower-level policy can never contradict a higher-level one.

## Tooling

- **`ubl` CLI** — Admin CLI (`crates/ubl`). Commands: `verify <receipt.json>`, `verify <cid>`.

- **`mc`** — MinIO client. Used for archiving release binaries to `lab512/ubl-releases`.

- **PM2** — Process manager. Runs `ubl-gate`, `deploy-webhook`, `cloudflared`, `minio`.
