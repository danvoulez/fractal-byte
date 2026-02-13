# UBL Runtime (Complete)

This workspace provides a **single, invocable TDLN** for all gates and services, plus an **Axum sidecar** for polyglot stacks.

## Layout
- `crates/ubl_tdln`: Core deterministic library (ρ/PURE canon, CID, D8 binding, execute).
- `services/ubl_tdln_svc`: HTTP sidecar exposing `/v1/execute`.
- `docs/BLUEPRINT.md`: Why/what/how, ops, conformance.

## Quickstart
```bash
# build & test
make build
make test

# run sidecar
make run
# then POST /v1/execute with a minimal manifest and vars (see docs)
```


## Addendum: Ghost Records & Write-Ahead/Write-After (2026‑02‑13)
This merge adds canonical docs for **Ghost Records** and **Write-Ahead/Write-After** as runtime contracts.

### Quick usage
```bash
# validate docs & examples
make docs
make validate-wa-wf
```

Artifacts added:
- `docs/runtime-ghost-write-ahead.md`
- `examples/wa.json`, `examples/wf.json`, `examples/receipt.json`
- `docs/CHANGELOG-addendum.md`

These specs formalize: 1 byte = 1 message = 1 CID (ρ/NRF), audit trail by receipts, privacy via capsules, and LLM‑first narration from receipts.
