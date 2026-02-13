#!/usr/bin/env bash
set -euo pipefail

BASE=${1:-http://localhost:3000}

echo "=== healthz ==="
curl -sf "$BASE/healthz" | python3 -m json.tool

echo "=== ingest + certify ==="
CID=$(curl -sf "$BASE/v1/ingest" \
  -H 'content-type: application/json' \
  -d '{"payload":{"hello":"world","n":42},"certify":true}' | python3 -c "import sys,json; print(json.load(sys.stdin)['cid'])")
echo "CID=$CID"

echo "=== raw bytes ==="
curl -sf -o /tmp/nrf.bin "$BASE/cid/$CID"
xxd -l 64 /tmp/nrf.bin

echo "=== json view ==="
curl -sf "$BASE/cid/$CID.json" | python3 -m json.tool

echo "=== receipt (JWS, truncated) ==="
curl -sf "$BASE/v1/receipt/$CID" | cut -c1-80
echo "..."

echo "=== DID ==="
curl -sf "$BASE/.well-known/did.json" | python3 -m json.tool

echo "=== determinism (3 runs) ==="
for i in 1 2 3; do
  curl -sf "$BASE/v1/ingest" \
    -H 'content-type: application/json' \
    -d '{"payload":{"hello":"world","n":42}}' | python3 -c "import sys,json; print(json.load(sys.stdin)['cid'])"
done | sort -u | tee /tmp/cids_uniq.txt
COUNT=$(wc -l < /tmp/cids_uniq.txt | tr -d ' ')
if [ "$COUNT" -eq 1 ]; then
  echo "PASS: deterministic (1 unique CID)"
else
  echo "FAIL: got $COUNT unique CIDs"
  exit 1
fi

echo "=== all checks passed ==="
