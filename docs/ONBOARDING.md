# UBL Onboarding Guide

> Step-by-step: from zero to first receipt.

## Prerequisites

- UBL gate running (`api.ubl.agency` or `localhost:3000`)
- Admin token with tenant-creation permissions
- `curl` or UBL SDK (TS/Py)

## Step 1: Create User (UBL Global Policy)

A user is a human or service identity. Created by an admin under UBL global policy.

```bash
curl -X POST https://api.ubl.agency/v1/admin/users \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "dan",
    "display_name": "Dan Voulez",
    "did": "did:key:z6Mk...",
    "email": "dan@danvoulez.com"
  }'
```

**Policy gate**: UBL global policy validates the request. Receipt is generated.

## Step 2: Create Tenant (UBL Global Policy)

A tenant is an organizational boundary. The user becomes the tenant owner.

```bash
curl -X POST https://api.ubl.agency/v1/admin/tenants \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "tenant_id": "acme-corp",
    "display_name": "Acme Corporation",
    "owner_user_id": "dan",
    "origins": ["https://app.acme.com"]
  }'
```

**Policy gate**: UBL global policy validates tenant creation. Receipt chains to user creation receipt.

## Step 3: Create Tenant Policy

The tenant owner defines rules for their organization.

```bash
curl -X POST https://api.ubl.agency/v1/admin/tenants/acme-corp/policy \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "version": "1",
    "rules": [
      {
        "id": "ACME_REQUIRE_BRAND",
        "condition": "inputs.brand_id != null",
        "action": "DENY",
        "reason": "brand_id is required"
      }
    ]
  }'
```

**Policy gate**: UBL global policy validates that the tenant policy does not contradict global rules.

## Step 4: Issue Token

Create a bearer token scoped to the tenant.

```bash
curl -X POST https://api.ubl.agency/v1/admin/tenants/acme-corp/tokens \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "client_id": "acme-web",
    "kid": "acme-web-001",
    "scopes": ["execute", "read"]
  }'
# Response: { "token": "ubl-acme-token-xyz..." }
```

## Step 5: First Submission (Tenant Policy)

The user submits their first execution using the tenant token.

```bash
curl -X POST https://api.ubl.agency/v1/execute \
  -H "Authorization: Bearer ubl-acme-token-xyz" \
  -H "Content-Type: application/json" \
  -d '{
    "manifest": {
      "pipeline": "acme-hello",
      "in_grammar": {
        "inputs": {"brand_id": "", "message": ""},
        "mappings": [],
        "output_from": "message"
      },
      "out_grammar": {
        "inputs": {"result": ""},
        "mappings": [],
        "output_from": "result"
      },
      "policy": {"allow": true}
    },
    "vars": {
      "brand_id": "acme-brand-001",
      "message": "Hello from Acme"
    }
  }'
```

**Response** (receipt-first):

```json
{
  "receipts": {
    "wa": { "body_cid": "b3:...", "parents": [] },
    "transition": { "body_cid": "b3:...", "parents": ["b3:<wa_cid>"] },
    "wf": { "body_cid": "b3:...", "parents": ["b3:<transition_cid>"] }
  },
  "tip_cid": "b3:<wf_cid>",
  "artifacts": { "result": "Hello from Acme" }
}
```

## Step 6: Verify

```bash
# Verify the receipt chain
ubl verify receipt.json

# Or via API
curl https://api.ubl.agency/v1/receipt/b3:<tip_cid> \
  -H "Authorization: Bearer ubl-acme-token-xyz"
```

## Onboarding Flow Diagram

```text
Admin Token
    │
    ▼
┌──────────┐   UBL Policy   ┌──────────┐   UBL Policy   ┌──────────┐
│  Create  │───────────────▶│  Create  │───────────────▶│  Create  │
│  User    │                │  Tenant  │                │  Policy  │
└──────────┘                └──────────┘                └──────────┘
                                                             │
                                                             ▼
                            ┌──────────┐  Tenant Policy  ┌──────────┐
                            │  Issue   │◀────────────────│  First   │
                            │  Token   │                 │  Submit  │
                            └──────────┘                 └──────────┘
                                                             │
                                                             ▼
                                                        Receipt Chain
                                                        (WA→T→WF)
```

## Common Errors

| Code | Meaning | Fix |
| --- | --- | --- |
| 401 | Missing or invalid token | Check `Authorization` header |
| 403 | Token not authorized for this tenant/kid | Verify token scopes and `allowed_kids` |
| 409 | Duplicate execution (idempotency) | Same input already processed; use existing receipt |
| 413 | Body too large | Reduce payload below 1 MiB (or tenant limit) |
| 415 | Wrong content type | Use `Content-Type: application/json` |
| 429 | Rate limited | Wait for `Retry-After` header duration |

## Next Steps

- **SDK**: use `executeReceiptFirst()` from the TS/Py SDK for typed access
- **Verify**: always verify receipts by CID, never by URL
- **Audit**: query `/v1/audit?tenant=acme-corp` for execution history
- **Policy**: refine tenant policy as requirements evolve
