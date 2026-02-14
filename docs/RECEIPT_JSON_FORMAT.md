# UBL Receipt JSON Format

> Canonical JSON structure for receipts. This is the contract between backend and UI.

## Full Receipt Response (`/v1/execute`)

```json
{
  "receipts": {
    "wa": {
      "t": "2026-02-14T08:00:00.000Z",
      "parents": [],
      "body": {
        "type": "ubl/run",
        "version": "1",
        "phase": "before",
        "ghost": true,
        "chip": { "cid": "b3:abc123..." },
        "inputs": { "vars": { "brand_id": "acme-001", "message": "hello" } },
        "env": { "caller": "did:key:z6Mk...", "ts": "2026-02-14T08:00:00Z" },
        "artifacts": { "input_cid": "b3:def456...", "output_cid": null }
      },
      "body_cid": "b3:wa_cid_here...",
      "sig": "eyJ...<JWS compact>",
      "kid": "runtime-001",
      "observability": {
        "latency_ms": 1,
        "stage": "wa"
      }
    },
    "transition": {
      "t": "2026-02-14T08:00:00.001Z",
      "parents": ["b3:wa_cid_here..."],
      "body": {
        "type": "ubl/transition",
        "version": "1",
        "from_layer": -1,
        "to_layer": 0,
        "preimage_raw_cid": "b3:raw_rb_output...",
        "rho_cid": "b3:canonical_nrf...",
        "witness": {
          "vm_tag": "rb-vm-v1",
          "bytecode_cid": "b3:bytecode...",
          "fuel_spent": 1200
        },
        "ghost": false
      },
      "body_cid": "b3:transition_cid_here...",
      "sig": "eyJ...<JWS compact>",
      "kid": "runtime-001",
      "observability": {
        "latency_ms": 2,
        "stage": "transition"
      }
    },
    "wf": {
      "t": "2026-02-14T08:00:00.003Z",
      "parents": ["b3:transition_cid_here..."],
      "body": {
        "type": "ubl/run",
        "version": "1",
        "phase": "after",
        "ghost": true,
        "decision": "ALLOW",
        "reason": null,
        "rule_id": null,
        "artifacts": {
          "input_cid": "b3:def456...",
          "output_cid": "b3:output789...",
          "sub_receipts": []
        }
      },
      "body_cid": "b3:wf_cid_here...",
      "sig": "eyJ...<JWS compact>",
      "kid": "runtime-001",
      "observability": {
        "latency_ms": 3,
        "stage": "wf",
        "policy_trace": [
          { "level": "global", "rule": "UBL_AUTH_REQUIRED", "result": "PASS" },
          { "level": "global", "rule": "UBL_MAX_BODY_1MB", "result": "PASS" }
        ],
        "timeline": [
          { "t": "2026-02-14T08:00:00.000Z", "verb": "INGEST" },
          { "t": "2026-02-14T08:00:00.001Z", "verb": "PARSE" },
          { "t": "2026-02-14T08:00:00.002Z", "verb": "POLICY", "decision": "ALLOW" },
          { "t": "2026-02-14T08:00:00.003Z", "verb": "RENDER" }
        ]
      }
    }
  },
  "tip_cid": "b3:wf_cid_here...",
  "artifacts": {
    "result": "Hello from Acme"
  }
}
```

## Receipt Fields

### Common Fields (all receipts)

| Field | Type | Description |
| --- | --- | --- |
| `t` | ISO 8601 string | Timestamp of receipt creation |
| `parents` | `string[]` | CIDs of parent receipts in the chain |
| `body` | object | Receipt-type-specific payload |
| `body_cid` | string | CID of the NRF-canonical body bytes |
| `sig` | string | JWS EdDSA detached signature (RFC 7797) |
| `kid` | string | Key ID of the signing key |
| `observability` | object | Latency, stage, timeline, policy trace |

### WA Body Fields (`phase: "before"`)

| Field | Type | Description |
| --- | --- | --- |
| `type` | `"ubl/run"` | Record type |
| `version` | `"1"` | Schema version |
| `phase` | `"before"` | Write-ahead marker |
| `ghost` | boolean | Always `true` for WA |
| `chip` | `{ cid }` | Reference to execution chip |
| `inputs` | `{ vars }` | Input variables |
| `env` | `{ caller, ts }` | Execution environment |
| `artifacts.input_cid` | string | CID of canonicalized inputs |
| `artifacts.output_cid` | null | Always null in WA |

### Transition Body Fields

| Field | Type | Description |
| --- | --- | --- |
| `type` | `"ubl/transition"` | Record type |
| `from_layer` | `-1` | Source layer (RB-VM) |
| `to_layer` | `0` | Target layer (ρ/canonical) |
| `preimage_raw_cid` | string | CID of raw RB-VM output |
| `rho_cid` | string | CID after NRF canonicalization |
| `witness.vm_tag` | string | VM identifier |
| `witness.bytecode_cid` | string | CID of executed bytecode |
| `witness.fuel_spent` | integer | Fuel consumed by execution |

### WF Body Fields (`phase: "after"`)

| Field | Type | Description |
| --- | --- | --- |
| `type` | `"ubl/run"` | Record type |
| `phase` | `"after"` | Write-after marker |
| `ghost` | boolean | Always `true` for WF |
| `decision` | `"ALLOW" \| "DENY"` | Execution outcome |
| `reason` | string or null | Human-readable reason (mandatory on DENY) |
| `rule_id` | string or null | Policy rule that decided (mandatory on DENY) |
| `artifacts.output_cid` | string or null | CID of output (null on DENY) |
| `artifacts.sub_receipts` | `string[]` | CIDs of sub-receipts (nested executions) |

### Observability Fields

| Field | Type | Description |
| --- | --- | --- |
| `latency_ms` | integer | Time spent in this stage |
| `stage` | string | Stage name (`wa`, `transition`, `wf`) |
| `timeline` | `[{t, verb, decision?}]` | Ordered execution events |
| `policy_trace` | `[{level, rule, result, reason?}]` | Policy evaluation log |

## UI Field Mapping

| UI Section | JSON Source |
| --- | --- |
| Status banner | `receipts.wf.body.decision` → PASS/DENY; absence of WF → RETRY |
| Status hint | `receipts.wf.body.reason` (DENY) or default message (PASS) |
| CID | `tip_cid` |
| URL | `{gate_base}/v1/receipt/{tip_cid}` |
| DID | `did:cid:{tip_cid}` |
| TIP CID | `receipts.wf.body_cid` |
| Stage pills | `receipts.*.observability.stage` + `latency_ms` |
| Stage hover | `receipts.*.body` (CIDs, fuel, inputs) |
| JWS verified | `receipts.wf.sig` + `receipts.wf.kid` |
| Chain count | `len(receipts)` (always 3: wa, transition, wf) |
| Parents | `receipts.wf.parents` |
| Narrative | Generated from `decision` + `reason` + `rule_id` + `timeline` |
| Policy trace | `receipts.wf.observability.policy_trace` |
| Raw JSON | Full response object |

## DENY Response Example

```json
{
  "receipts": {
    "wa": { "body_cid": "b3:...", "parents": [] },
    "transition": { "body_cid": "b3:...", "parents": ["b3:..."] },
    "wf": {
      "body": {
        "phase": "after",
        "decision": "DENY",
        "reason": "Payload exceeds Acme's 512KB limit",
        "rule_id": "ACME_MAX_PAYLOAD",
        "artifacts": { "output_cid": null }
      },
      "body_cid": "b3:...",
      "observability": {
        "policy_trace": [
          { "level": "global", "rule": "UBL_MAX_BODY_1MB", "result": "PASS" },
          { "level": "tenant", "rule": "ACME_MAX_PAYLOAD", "result": "DENY", "reason": "Payload exceeds Acme's 512KB limit" }
        ]
      }
    }
  },
  "tip_cid": "b3:...",
  "artifacts": null
}
```

## Schema References

- Receipt: `schemas/receipt.schema.json`
- Transition: `schemas/transition.schema.json`
- WA/WF: defined in `docs/runtime-ghost-write-ahead.md`
