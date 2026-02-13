#!/usr/bin/env bash
set -euo pipefail
BASE=${BASE:-http://localhost:3000}
echo "[1/4] healthz"
curl -fsS "$BASE/healthz" >/dev/null || { echo "health failed"; exit 1; }

echo "[2/4] ingest + certify (returns tip receipt)"
OUT=$(curl -fsS "$BASE/v1/ingest" -H 'content-type: application/json'   -d '{"payload":{"hello":"world","n":42},"certify":true}')
echo "$OUT" | jq . >/dev/null
TIP=$(echo "$OUT" | jq -r .tip_cid)
test -n "$TIP"

echo "[3/4] receipt chain has -1→0 transition"
curl -fsS "$BASE/v1/receipt/$TIP" | jq '..|select(type=="object" and .t?=="ubl/transition")|.t' -r | grep -q '^ubl/transition$'

echo "[4/4] determinism on execute"
REQ='{"manifest":{"name":"hello"},"vars":{"x":1}}'
A=$(curl -fsS "$BASE/v1/execute" -H 'content-type: application/json' -d "$REQ" | jq -r .tip_cid)
B=$(curl -fsS "$BASE/v1/execute" -H 'content-type: application/json' -d "$REQ" | jq -r .tip_cid)
test "$A" = "$B"
echo "OK"
