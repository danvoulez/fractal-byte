# UBL Security

> Authentication, authorization, CORS, edge protections, and audit.

## Authentication (AuthN)

### Bearer Tokens

All API calls (except `/healthz` and `/metrics`) require a bearer token:

```text
Authorization: Bearer <token>
```

Tokens are stored in `config/seed_tokens.json` and loaded at gate startup. Each token maps to:

```json
{
  "token": "ubl-acme-token-001",
  "client_id": "acme-web",
  "kid": "acme-web-001",
  "allowed_kids": ["acme-web-001"],
  "scopes": ["execute", "read"],
  "tenant_id": "acme-corp"
}
```

### Token Resolution Flow

```text
Request arrives
  → Extract Authorization header
  → Look up token in TokenStore
  → Build ClientInfo { client_id, kid, allowed_kids, tenant_id }
  → Inject ClientInfo into request extensions
  → Downstream handlers access via Option<Extension<ClientInfo>>
```

### Response Codes

| Code | Meaning |
| --- | --- |
| 401 | Missing or invalid token |
| 403 | Token valid but not authorized (kid mismatch, scope mismatch) |

### Dev Token

For local development, set `UBL_DEV_TOKEN` in `.env`. When `UBL_AUTH_DISABLED=1`, all requests are accepted (never use in production).

## Authorization (AuthZ)

### KID-Scoped Access

Each token has `allowed_kids` — a list of key IDs the client may use. When a request references a `kid`:

```text
ClientInfo.kid_allowed(requested_kid) → true/false
```

Mismatch → 403 FORBIDDEN with receipt:

```json
{
  "decision": "DENY",
  "reason": "kid acme-web-002 not in allowed_kids for client acme-web",
  "rule_id": "UBL_KID_SCOPE"
}
```

### Scope Enforcement

Tokens declare scopes (`execute`, `read`, `admin`). Endpoints check scope before processing:

| Endpoint | Required Scope |
| --- | --- |
| `POST /v1/execute` | `execute` |
| `GET /v1/receipt/:cid` | `read` |
| `GET /v1/transition/:cid` | `read` |
| `POST /v1/admin/*` | `admin` |
| `GET /v1/audit` | `read` or `admin` |

## Edge Protections

The gate enforces hard limits before any business logic:

| Protection | Limit | Response |
| --- | --- | --- |
| Body size | 1 MiB | 413 Payload Too Large |
| Request timeout | 30 seconds | 408 Request Timeout |
| Content-Type | `application/json` required | 415 Unsupported Media Type |
| Rate limit | Per client_id (configurable) | 429 Too Many Requests + `Retry-After` |

These are UBL global policy — tenants cannot weaken them (but can set stricter limits).

## CORS

### Current Configuration

Whitelist-based CORS via `tower-http::CorsLayer`:

**Allowed origins**:

- `https://*.ubl.agency`
- `http://localhost:*` (dev only)

**Allowed methods**: GET, POST, PUT, DELETE, OPTIONS

**Allowed headers**: Content-Type, Authorization, X-UBL-Tenant, X-UBL-Kid

**Exposed headers**: X-RateLimit-Limit, X-RateLimit-Remaining, X-RateLimit-Reset

### Per-Tenant CORS (planned)

Each tenant will have its own allowed origins:

```json
{
  "tenant_id": "acme-corp",
  "origins": ["https://app.acme.com", "https://staging.acme.com"]
}
```

The gate will resolve tenant from the bearer token and apply tenant-specific CORS. Mismatched origin → blocked preflight.

## Rate Limiting

### Current

Global rate limiter: 600 requests/minute with configurable burst.

### Planned (PR I2)

Per-client_id token bucket:

```text
Request arrives
  → Resolve client_id from token
  → Check bucket for client_id
  → If tokens available → proceed, decrement
  → If empty → 429 with Retry-After header + RETRY receipt
```

Headers on every response:

```text
X-RateLimit-Limit: 600
X-RateLimit-Remaining: 542
X-RateLimit-Reset: 1707897600
```

## Key Rotation

The gate's `KeyRing` manages signing keys:

- Active key signs new receipts
- Previous keys remain valid for verification
- Rotation: generate new key → set as active → old key enters grace period
- `kid` in receipt points to the signing key's DID

## Threat Model

| Threat | Mitigation |
| --- | --- |
| Token theft | Short-lived tokens, kid-scoping, tenant isolation |
| Replay attack | 409 CONFLICT on duplicate execution (idempotency by CID) |
| Body tampering | CID verification — any change breaks the hash |
| Receipt forgery | JWS EdDSA signature with DID-published key |
| Cross-tenant access | Token→tenant binding, 403 on mismatch |
| DDoS | Rate limiting per client_id, body size limit, timeout |
| CORS bypass | Strict origin whitelist, no wildcard in production |
| Man-in-middle | TLS via Cloudflare Tunnel (end-to-end encryption) |

## Implementation References

| Component | File |
| --- | --- |
| Auth middleware | `services/ubl_gate/src/lib.rs` (auth layer, TokenStore, ClientInfo) |
| KID scope check | `services/ubl_gate/src/api.rs` (kid_allowed) |
| CORS layer | `services/ubl_gate/src/lib.rs` (CorsLayer) |
| Edge limits | `services/ubl_gate/src/lib.rs` (RequestBodyLimitLayer, TimeoutLayer) |
| Rate limiter | `services/ubl_gate/src/lib.rs` (RateLimiter) |
| JWS signing | `crates/ubl_runtime/src/jws.rs` |
| Token config | `config/seed_tokens.json` |
| Tests | `services/ubl_gate/tests/hardened.rs` (32 tests) |
