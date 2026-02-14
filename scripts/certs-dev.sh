#!/usr/bin/env bash
# UBL — Generate dev mTLS certificates (CA, server, client)
# Usage: bash scripts/certs-dev.sh [output_dir]
# Requires: openssl
set -euo pipefail

OUT="${1:-certs/dev}"
DAYS=365
CN_CA="UBL Dev CA"
CN_SERVER="ubl-gate"
CN_CLIENT="ubl-ledger-client"

mkdir -p "$OUT"

echo "── Generating CA ──"
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
  -keyout "$OUT/ca.key" -out "$OUT/ca.crt" -days $DAYS -nodes \
  -subj "/CN=$CN_CA/O=UBL Dev"

echo "── Generating server cert (gate) ──"
openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
  -keyout "$OUT/server.key" -out "$OUT/server.csr" -nodes \
  -subj "/CN=$CN_SERVER/O=UBL Dev"
openssl x509 -req -in "$OUT/server.csr" -CA "$OUT/ca.crt" -CAkey "$OUT/ca.key" \
  -CAcreateserial -out "$OUT/server.crt" -days $DAYS \
  -extfile <(printf "subjectAltName=DNS:localhost,DNS:ubl-gate,IP:127.0.0.1")

echo "── Generating client cert (ledger/S3 client) ──"
openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
  -keyout "$OUT/client.key" -out "$OUT/client.csr" -nodes \
  -subj "/CN=$CN_CLIENT/O=UBL Dev"
openssl x509 -req -in "$OUT/client.csr" -CA "$OUT/ca.crt" -CAkey "$OUT/ca.key" \
  -CAcreateserial -out "$OUT/client.crt" -days $DAYS

# Clean up CSRs
rm -f "$OUT/server.csr" "$OUT/client.csr"

echo ""
echo "✓ Certificates generated in $OUT/"
echo "  ca.crt / ca.key       — Certificate Authority"
echo "  server.crt / server.key — Gate server (TLS termination)"
echo "  client.crt / client.key — Ledger/S3 client (mTLS)"
echo ""
echo "Usage:"
echo "  Server: --tls-cert $OUT/server.crt --tls-key $OUT/server.key --tls-ca $OUT/ca.crt"
echo "  Client: --tls-cert $OUT/client.crt --tls-key $OUT/client.key --tls-ca $OUT/ca.crt"
