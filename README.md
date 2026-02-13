# UBL — Universal Business Leverage

**Contato:** dan@ubl.agency

## Crates

| Crate | Papel |
|-------|-------|
| `ubl_ai_nrf1` | NRF-1.1 codec + CID (ρ byte layer) |
| `ubl_runtime` | Deterministic engine: parse → policy → render (D8 binding) |
| `ubl_ledger` | Content-addressed storage (FS; pronto p/ S3) |
| `ubl_receipt` | JWS Ed25519 receipts + DID |
| `ubl_did` | DID document + did:cid resolution |
| `ubl_config` | BASE_URL e parâmetros |

## Service

| Service | Papel |
|---------|-------|
| `ubl_gate` | Policy gate — single HTTP entry point (`:3000`) |

## Comandos

```bash
make build        # build workspace
make test         # 66 tests across all crates
make run          # start gate on :3000
make race         # race card end-to-end test
make conformance  # ubl_runtime determinism tests
make validate     # full test + CID check
```

## Exemplo

```bash
# Ingest + certify
curl -s http://localhost:3000/v1/ingest \
  -H 'content-type: application/json' \
  -d '{"payload":{"hello":"world","n":42},"certify":true}' | jq

# Use the CID from the response
CID="bafkreigibhh2eucyeberwvkqx56braqzvokd2d45jrg24d5iqcsoumjmrq"

# Raw NRF bytes
curl -s http://localhost:3000/cid/$CID -o /dev/null -w "%{http_code}\n"

# JSON view
curl -s http://localhost:3000/cid/$CID.json | jq

# Receipt (JWS)
curl -s http://localhost:3000/v1/receipt/$CID

# Execute runtime (deterministic)
curl -s http://localhost:3000/v1/execute \
  -H 'content-type: application/json' \
  -d '{"manifest":{"pipeline":"hello","in_grammar":{"inputs":{"raw_b64":""},"mappings":[{"from":"raw_b64","codec":"base64.decode","to":"raw.bytes"}],"output_from":"raw.bytes"},"out_grammar":{"inputs":{"content":""},"mappings":[],"output_from":"content"},"policy":{"allow":true}},"vars":{"input_data":"aGVsbG8="}}' | jq

# DID document
curl -s http://localhost:3000/.well-known/did.json | jq
```
