# UBL Audit Runbook

> How to audit executions, verify receipt chains, and investigate anomalies.
> All auditing is receipt-based — replay by CIDs, never by URLs.

## Principles

1. **Receipts are the single source of truth**. The ledger stores them; the audit reads them.
2. **Replay by CID**: to re-verify an execution, follow the parents→tip chain using CIDs. Never rely on URLs (they are transport).
3. **Tenant-scoped**: audit queries are always scoped to a tenant. Global audit requires an admin token.
4. **Immutable chain**: if any receipt in the chain has a CID mismatch, the chain is broken and the audit flags it.

## Audit Query API

### GET `/v1/audit`

Query executions by tenant and time window.

```bash
curl "https://api.ubl.agency/v1/audit?tenant=acme-corp&from=2026-02-01T00:00:00Z&to=2026-02-14T23:59:59Z" \
  -H "Authorization: Bearer $ADMIN_TOKEN"
```

**Parameters**:

| Param | Type | Required | Description |
| --- | --- | --- | --- |
| `tenant` | string | yes | Tenant ID to audit |
| `from` | ISO 8601 | yes | Start of time window |
| `to` | ISO 8601 | yes | End of time window |
| `decision` | string | no | Filter: `ALLOW`, `DENY`, or `RETRY` |
| `limit` | integer | no | Max results (default 100, max 1000) |
| `cursor` | string | no | Pagination cursor (CID of last result) |

**Response**:

```json
{
  "tenant": "acme-corp",
  "window": { "from": "2026-02-01T00:00:00Z", "to": "2026-02-14T23:59:59Z" },
  "summary": {
    "total": 1247,
    "pass": 1180,
    "deny": 52,
    "retry": 15,
    "avg_latency_ms": 12,
    "p99_latency_ms": 45
  },
  "anomalies": [
    {
      "type": "latency_outlier",
      "cid": "b3:abc123...",
      "latency_ms": 890,
      "threshold_ms": 100
    },
    {
      "type": "chain_broken",
      "cid": "b3:def456...",
      "expected_parent": "b3:ghi789...",
      "actual_parent": null
    }
  ],
  "executions": [
    {
      "tip_cid": "b3:wf_cid...",
      "decision": "ALLOW",
      "t": "2026-02-14T08:00:00Z",
      "latency_ms": 8,
      "chain_length": 3,
      "chain_valid": true
    }
  ],
  "next_cursor": "b3:last_cid..."
}
```

## Chain Verification

### Manual (CLI)

```bash
# Verify a single receipt
ubl verify receipt.json

# Verify by CID (fetches from gate)
ubl verify b3:wf_cid_here...

# Output:
# ✓ body_cid matches
# ✓ JWS signature valid (kid: runtime-001)
# ✓ parents chain: WA → Transition → WF
# ✓ transition: layer -1 → 0 (fuel: 1200)
```

### Programmatic (Replay)

To fully re-verify an execution:

```text
1. Fetch WF receipt by tip_cid
2. Extract parents[] → fetch Transition receipt
3. Extract parents[] → fetch WA receipt
4. For each receipt:
   a. Recompute body_cid from NRF(body) → must match stored body_cid
   b. Verify JWS signature against kid's public key (DID resolution)
   c. Verify parent CIDs match the chain
5. For Transition:
   a. Verify preimage_raw_cid → NRF → rho_cid matches
   b. Verify witness (vm_tag, bytecode_cid, fuel_spent)
```

**Critical**: replay uses CIDs to fetch data, never URLs. If the CAS (content-addressed storage) returns bytes whose hash doesn't match the CID, the verification fails.

## Anomaly Detection

The audit system flags these anomalies:

| Anomaly | Detection | Severity |
| --- | --- | --- |
| `chain_broken` | Parent CID in receipt doesn't match any stored receipt | Critical |
| `cid_mismatch` | Recomputed body_cid doesn't match stored body_cid | Critical |
| `sig_invalid` | JWS signature verification fails | Critical |
| `latency_outlier` | Execution latency > p99 × 3 | Warning |
| `deny_spike` | DENY rate > 2× rolling average | Warning |
| `ghost_leak` | Ghost record appeared in external telemetry | Warning |
| `kid_unknown` | Receipt signed with kid not in known key set | Critical |

## Audit Report Format

### Execution Audit (per-tenant, per-window)

```text
═══════════════════════════════════════════════
  UBL Audit Report — acme-corp
  Window: 2026-02-01 → 2026-02-14
═══════════════════════════════════════════════

  Summary
  ───────
  Total executions:  1,247
  PASS:              1,180 (94.6%)
  DENY:                 52 (4.2%)
  RETRY:                15 (1.2%)

  Latency
  ───────
  Average:           12 ms
  P50:                8 ms
  P99:               45 ms
  Max:              890 ms (⚠ outlier)

  Chain Integrity
  ───────────────
  Verified:        1,245 / 1,247
  Broken:              2 (⚠ investigate)

  Anomalies (2)
  ─────────────
  1. chain_broken  b3:def456...  missing parent
  2. latency_outlier  b3:abc123...  890ms (threshold: 100ms)

  Top DENY Reasons
  ────────────────
  ACME_MAX_PAYLOAD:     28 (53.8%)
  ACME_REQUIRE_BRAND:   18 (34.6%)
  UBL_RATE_LIMIT:        6 (11.5%)
═══════════════════════════════════════════════
```

## Runbook: Investigating a Broken Chain

```text
1. Identify the broken receipt:
   GET /v1/receipt/b3:def456...

2. Check its parents:
   → parents: ["b3:ghi789..."]
   → GET /v1/receipt/b3:ghi789... → 404 (missing!)

3. Check if parent exists in MinIO archive:
   mc ls lab512/ubl-data/tenants/acme-corp/receipts/b3:ghi789...

4. If found in MinIO but not in gate memory:
   → Gate restarted and lost in-memory receipts
   → Action: restore from MinIO or S3 ledger

5. If not found anywhere:
   → Receipt was never written (crash between WA and WF?)
   → Action: check PM2 logs for crash at that timestamp
   → Action: flag execution as incomplete in audit
```

## Runbook: Investigating a DENY Spike

```text
1. Query DENYs in the window:
   GET /v1/audit?tenant=acme-corp&decision=DENY&from=...&to=...

2. Group by rule_id:
   → If one rule dominates: check if policy was recently changed
   → If spread across rules: check if inputs changed (new client, new data format)

3. Check policy_trace in sample receipts:
   → Which level denied? (global vs tenant vs app)
   → Was the deny correct per policy?

4. If false positive:
   → Update tenant policy
   → Re-execute affected inputs (new receipts, new chain)
```

## Automation

### Scheduled Audit (cron / PM2)

```bash
# Daily audit report for all tenants
0 6 * * * curl -sf "https://api.ubl.agency/v1/audit/report?window=24h" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -o /var/log/ubl/audit-$(date +%Y%m%d).json
```

### Alerting

Configure alerts for critical anomalies:

- `chain_broken` → immediate alert (Slack/email)
- `sig_invalid` → immediate alert
- `deny_spike` → warning (threshold: 2× rolling average)
- `latency_outlier` → warning (threshold: p99 × 3)
