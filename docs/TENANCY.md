# UBL Tenancy & Policy Cascade

> Multi-tenant by design. Policies compose hierarchically — never contradict upward.

## Tenant Model

A **tenant** is an isolated organizational unit within UBL. Each tenant has:

- **Identity**: unique `tenant_id` (slug), display name
- **Tokens**: bearer tokens scoped to the tenant, each with `client_id` and `allowed_kids`
- **Policies**: tenant-level rules that extend (never contradict) UBL global policy
- **Origins**: allowed CORS origins for this tenant's API calls
- **Quotas**: rate limits, storage limits, execution limits

```json
{
  "tenant_id": "acme-corp",
  "display_name": "Acme Corporation",
  "created_at": "2026-02-14T08:00:00Z",
  "policy_ref": "b3:<tenant_policy_cid>",
  "tokens": [
    {
      "client_id": "acme-web",
      "kid": "acme-web-001",
      "allowed_kids": ["acme-web-001"],
      "scopes": ["execute", "read"]
    }
  ],
  "origins": ["https://app.acme.com", "https://staging.acme.com"],
  "quotas": {
    "rpm": 600,
    "burst": 50,
    "max_body_bytes": 1048576,
    "storage_mb": 1024
  }
}
```

## Policy Cascade

Policies resolve in strict hierarchical order:

```text
┌─────────────────────────────┐
│  UBL Global Policy          │  ← Cannot be overridden
│  (system invariants)        │
├─────────────────────────────┤
│  Tenant Policy              │  ← Extends global; cannot contradict
│  (org-level rules)          │
├─────────────────────────────┤
│  App / Project Policy       │  ← Extends tenant; cannot contradict
│  (optional, per-project)    │
└─────────────────────────────┘
```

### Resolution Rules

1. **Global always wins**: UBL global policy defines hard invariants (e.g., max body size, required auth, No-IO law). No tenant or app can weaken these.

2. **Tenant extends**: a tenant policy can add restrictions (e.g., stricter rate limits, additional required fields) but never remove global restrictions.

3. **App extends**: an app/project policy can further restrict within the tenant's bounds.

4. **Conflict → DENY**: if a lower-level policy contradicts a higher-level one, the execution is denied with a receipt explaining which rule conflicted.

### Policy Document

```json
{
  "version": "1",
  "level": "tenant",
  "tenant_id": "acme-corp",
  "rules": [
    {
      "id": "ACME_REQUIRE_BRAND",
      "description": "All executions must include brand_id in inputs",
      "condition": "inputs.brand_id != null",
      "action": "DENY",
      "reason": "brand_id is required for all Acme executions"
    },
    {
      "id": "ACME_MAX_PAYLOAD",
      "description": "Acme limits payloads to 512KB",
      "condition": "body_size <= 524288",
      "action": "DENY",
      "reason": "Payload exceeds Acme's 512KB limit"
    }
  ],
  "inherits": "ubl:global:v1"
}
```

### Policy Trace in Receipt

When a policy decision is made, the receipt's `observability.policy_trace` records which rules were evaluated and which decided:

```json
{
  "observability": {
    "policy_trace": [
      {"level": "global", "rule": "UBL_AUTH_REQUIRED", "result": "PASS"},
      {"level": "global", "rule": "UBL_MAX_BODY_1MB", "result": "PASS"},
      {"level": "tenant", "rule": "ACME_REQUIRE_BRAND", "result": "PASS"},
      {"level": "tenant", "rule": "ACME_MAX_PAYLOAD", "result": "DENY", "reason": "Payload exceeds Acme's 512KB limit"}
    ],
    "decided_by": "ACME_MAX_PAYLOAD",
    "decision": "DENY"
  }
}
```

## Isolation Guarantees

| Boundary | Guarantee |
| --- | --- |
| Data | Tenant A cannot read tenant B's receipts, transitions, or artifacts |
| Tokens | Tokens are scoped to a single tenant; cross-tenant calls → 403 |
| Rate limits | Per-tenant buckets; one tenant's burst cannot starve another |
| CORS | Origins are per-tenant; mismatched origin → blocked preflight |
| Policies | Tenant policies cannot weaken global; app policies cannot weaken tenant |
| Audit | Audit queries are tenant-scoped; global audit requires admin token |

## Storage Layout (MinIO / S3)

```text
ubl-data/
├── tenants/
│   ├── acme-corp/
│   │   ├── receipts/          CID-addressed receipts
│   │   ├── transitions/       Transition receipts
│   │   ├── artifacts/         Output artifacts
│   │   └── policy/            Policy documents (versioned)
│   └── beta-inc/
│       └── ...
└── global/
    ├── policy/                UBL global policy
    └── keys/                  Public keys for DID resolution
```

## API Headers

Tenant context is conveyed via headers:

| Header | Purpose | Example |
| --- | --- | --- |
| `Authorization` | Bearer token (maps to client_id + tenant) | `Bearer ubl-acme-token-001` |
| `X-UBL-Tenant` | Explicit tenant override (admin only) | `acme-corp` |
| `X-UBL-Kid` | Key ID for scoped access | `acme-web-001` |

The gate resolves tenant from the bearer token's `client_id`. The `X-UBL-Tenant` header is only honored for admin tokens.
