# mTLS Internal — Runbook

## Overview

mTLS secures internal communication between the UBL gate and backend services (ledger/S3).
In dev mode, the local filesystem ledger doesn't require mTLS. In staging/prod, S3 connections
use AWS SDK's built-in TLS; mTLS is for service-to-service calls when deploying ledger as a
separate network service.

## Certificate Generation

```bash
# Generate dev certs (CA + server + client)
make certs-dev
# or directly:
bash scripts/certs-dev.sh certs/dev
```

Output:

- `certs/dev/ca.crt` / `ca.key` — Certificate Authority
- `certs/dev/server.crt` / `server.key` — Gate server
- `certs/dev/client.crt` / `client.key` — Ledger client

## Environment Variables

```env
# Enable mTLS (default: off in dev)
UBL_MTLS_ENABLED=0

# Certificate paths
UBL_TLS_CA=certs/dev/ca.crt
UBL_TLS_CERT=certs/dev/server.crt
UBL_TLS_KEY=certs/dev/server.key
UBL_CLIENT_CERT=certs/dev/client.crt
UBL_CLIENT_KEY=certs/dev/client.key
```

## Verification

### Without client cert → connection refused

```bash
curl --cacert certs/dev/ca.crt https://localhost:3443/healthz
# Expected: SSL handshake error (no client cert)
```

### With valid client cert → success

```bash
curl --cacert certs/dev/ca.crt \
     --cert certs/dev/client.crt \
     --key certs/dev/client.key \
     https://localhost:3443/healthz
# Expected: {"ok": true}
```

## Rotation

1. Generate new CA + certs: `bash scripts/certs-dev.sh certs/new`
2. Deploy new CA to all services (grace period: both old and new CA trusted)
3. Deploy new server/client certs
4. Remove old CA from trust store

## Alerts

- `gate_mtls_handshake_failed_total` — client cert validation failures
- `gate_mtls_cert_expiry_days` — days until cert expiry (alert < 30)
