# MERGE NOTES — Receipt-first integration

## Breaking behavior
- `/v1/ingest` and `/v1/execute` now **always** persist and return receipts:
  ```json
  { "tip_receipt": { ... }, "tip_cid": "b3:...", "url": ".../v1/receipt/<cid>", "did": "did:key:..." }
  ```
- Transition receipt `ubl/transition` is **mandatory** whenever raw inputs are normalized.

## Rollback
- Revert this branch or skip the gate/runtime patches if you need legacy behavior.

## Observability
- Receipt `observability` fields are non-ID-bearing and do **not** affect content CIDs.
