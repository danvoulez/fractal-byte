#!/usr/bin/env bash
set -euo pipefail

# CID determinism check: ingest the same payload twice via ubl_runtime
# and verify the CID is identical both times.

echo "=== CID determinism check (ubl_runtime) ==="
cargo test -p ubl_runtime -- determinism_10x --quiet 2>&1 | tail -3

echo "=== CID determinism check (ubl_gate race card) ==="
cargo test -p ubl_gate --test race_card -- --quiet 2>&1 | tail -3

echo "=== all CID checks passed ==="
