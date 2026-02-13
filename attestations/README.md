# Attestations
Formato mínimo assinado para afirmar fatos sobre um `target_cid`.

Schema: `schemas/attestation.schema.json`

Exemplo:
```json
{
  "type": "attestation",
  "target_cid": "cidv1-raw-sha2-256:EXAMPLE",
  "claim": "passed_tests",
  "evidence": ["cidv1-raw-sha2-256:VECTORSET"],
  "signer": "@org/ci",
  "created_at": "2026-02-13T00:00:00Z",
  "signature": "base64:TODO"
}
```
