# UBL Architecture

> Registry + Gate as personal and multi-tenant infrastructure.
> A single fractal pipeline from smallest to largest, with chained receipts at every hop.

## Layers

```
Layer   Name          Crate                 Purpose
─────   ────          ─────                 ───────
 -1     ubl-rb        crates/rb_vm          Reasoning Bytecode VM. Deterministic, no IO.
  0     ρ/byte        crates/ubl_ai_nrf1    NRF-1.1 canonicalization, CID generation (BLAKE3).
  0+    WA/WF         crates/ubl_runtime    Write-Ahead / Write-After. Decision and outputs.
  ∞     Gate          services/ubl_gate     HTTP surface. Auth, edge, CORS, rate-limit.
```

### Layer -1: RB-VM (`crates/rb_vm`)

Deterministic stack machine. 19 opcodes, TLV bytecode, fuel metering.
No IO by construction — the VM can only interact via CAS reads and signature providers (`CanonProvider` trait).

- **Input**: bytecode (TLV) + stack arguments
- **Output**: stack result + fuel_spent
- **Spec**: `docs/spec/RB_VM_DECISIONS.md` (D1–D6), `docs/spec/RB_VM_LAWS.md` (10 laws)

### Layer 0: ρ/byte (`crates/ubl_ai_nrf1`)

NRF-1.1 canonical form: sorted keys by codepoint, NFC strings, null-stripping, BOM removal, i64-only numbers.
Every value is canonicalized before hashing → `b3:<hex64>` CID (BLAKE3).

- **Invariant**: same logical value → same bytes → same CID, always.

### Layer 0+: WA/WF (`crates/ubl_runtime`)

The execution pipeline. Every invocation produces:

1. **WA (Write-Ahead)**: materializes intent before execution (inputs, chip, policy, environment). Ghost by default.
2. **Transition -1→0**: proves the RB → ρ jump (preimage_raw_cid → rho_cid) with witness (vm tag, bytecode_cid, fuel_spent).
3. **Execute**: `bind → parse → policy → render|deny`.
4. **WF (Write-After)**: materializes result after execution (decision, artifacts, output_cid). Ghost by default.

The **receipt** chains WA → Transition → WF with JWS EdDSA proof.

### Layer ∞: Gate (`services/ubl_gate`)

HTTP surface on `:3000`. Responsibilities:

- AuthN/Z (bearer tokens, kid-scoped)
- Edge protections (1 MiB body limit, 30s timeout, content-type enforcement)
- CORS (whitelist per origin)
- Rate limiting
- Receipt storage (in-memory, future: S3 ledger)

## The Single Pipeline Law

**Every entry follows the same path:**

```
input → rb(-1) → ρ(0) → WA → execute → WF → receipt
```

Each passage generates or extends a receipt (chained by parent CIDs).
There are no shortcuts, no alternative paths. The pipeline is the same whether the input is a 10-byte string or a 10 MB document.

```
┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐
│  RB VM  │───▶│  NRF/ρ  │───▶│   WA    │───▶│ Execute │───▶│   WF    │
│ layer-1 │    │ layer 0 │    │ ghost   │    │ runtime │    │ ghost   │
└─────────┘    └─────────┘    └─────────┘    └─────────┘    └─────────┘
     │              │              │              │              │
     └──────────────┴──────────────┴──────────────┴──────────────┘
                          Receipt (chained, signed)
```

## Receipt as State

The receipt IS the state — not a side effect, not a log entry.

- **No RID**: identity comes from CID/DID/URL and parents→tip chain.
- **Transition -1→0 is mandatory**: preimage→ρ must be provable for auditability.
- **The ledger is a consequence**: first you write the receipt, then the ledger stores it. The receipt can never fail; the ledger can.
- **URL is transport; verification is by content (CID)**.

## Determinism Contract

| Question | Answer |
|---|---|
| Same input tomorrow? | → Runtime (deterministic) |
| Depends on world/time? | → Wasm acquires; runtime seals |

**Rules:**
- No IO in the runtime. IO only via Wasm, and always frozen by CID before use.
- Same input ⇒ same output ⇒ same CID.
- Each hop generates/extends a receipt; the ledger may fail, the receipt must not.

## Crate Map

```
crates/
├── rb_vm/              RB-VM: deterministic stack machine (19 opcodes, TLV, fuel)
├── ubl/                CLI: verify, publish
├── ubl_ai_nrf1/        NRF-1.1 canonicalization
├── ubl_config/         Configuration loading
├── ubl_did/            DID resolution
├── ubl_ledger/         Ledger abstraction (in-memory + S3 feature-gated)
├── ubl_receipt/        Receipt signing/verification (JWS EdDSA)
└── ubl_runtime/        Engine: execute, receipts, transition, jws, nrf_canon, rb_bridge

services/
└── ubl_gate/           HTTP gate: auth, edge, CORS, rate-limit, API endpoints
```

## Endpoints (Gate)

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/v1/execute` | Receipt-first execution (WA→Transition→WF) |
| `POST` | `/v1/execute/rb` | RB chip execution with transition receipt |
| `GET` | `/v1/transition/:cid` | Retrieve transition receipt by CID |
| `GET` | `/v1/receipt/:cid` | Retrieve receipt by CID |
| `GET` | `/v1/logline` | LLM-friendly logline |
| `GET` | `/healthz` | Health check |
| `GET` | `/metrics` | Prometheus metrics |

Full spec: `docs/OPENAPI.md`

## Infrastructure

```
git tag v* → push
  → GitHub Actions (gate + build macOS arm64 + changelog)
  → GitHub Release
  → Webhook → webhooks.ubl.agency
  → Download binary → bin/ubl_gate
  → PM2 restart → health check ✓
```

PM2 manages: `ubl-gate`, `deploy-webhook`, `cloudflared`, `minio`.
Cloudflare Tunnel exposes: `api.ubl.agency`, `webhooks.ubl.agency`, `minio.ubl.agency`, etc.

See `infra/ecosystem.config.js` and `infra/cloudflared.yml`.
