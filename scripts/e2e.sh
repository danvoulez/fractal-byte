#!/usr/bin/env bash
# UBL Gate — Rigorous Smoke Test (8 asserts, <5s)
# Requires: gate running on $BASE (default localhost:3000), jq
set -uo pipefail
BASE=${BASE:-http://localhost:3000}
PASS=0; FAIL=0

ok()   { echo "  ✓ $1"; PASS=$((PASS+1)); }
fail() { echo "  ✗ $1"; FAIL=$((FAIL+1)); }

MANIFEST='{"pipeline":"echo","in_grammar":{"inputs":{"raw_b64":""},"mappings":[{"from":"raw_b64","codec":"base64.decode","to":"raw.bytes"}],"output_from":"raw.bytes"},"out_grammar":{"inputs":{"content":""},"mappings":[],"output_from":"content"},"policy":{"allow":true}}'
EXEC_REQ="{\"manifest\":$MANIFEST,\"vars\":{\"input_data\":\"aGVsbG8=\"}}"

echo "=== UBL Gate Smoke Test ==="
echo ""

# 1) Healthz
echo "[1/8] healthz"
HZ=$(curl -fsS "$BASE/healthz" 2>/dev/null || echo '{}')
if [ "$(echo "$HZ" | jq -r .ok)" = "true" ]; then ok "healthz returns ok:true"; else fail "healthz returns ok:true"; fi

# 2) Execute returns receipt chain
echo "[2/8] execute returns receipt chain"
EXEC=$(curl -fsS "$BASE/v1/execute" -H 'content-type: application/json' -d "$EXEC_REQ" 2>/dev/null || echo '{}')
WA_T=$(echo "$EXEC" | jq -r '.receipts.wa.t // empty')
TR_T=$(echo "$EXEC" | jq -r '.receipts.transition.t // empty')
WF_T=$(echo "$EXEC" | jq -r '.receipts.wf.t // empty')
if [ "$WA_T" = "ubl/wa" ] && [ "$TR_T" = "ubl/transition" ] && [ "$WF_T" = "ubl/wf" ]; then
  ok "execute has receipts.wa, .transition, .wf"
else
  fail "execute has receipts.wa, .transition, .wf (got wa=$WA_T tr=$TR_T wf=$WF_T)"
fi

# 3) Transition has from/to CIDs
echo "[3/8] transition receipt has preimage_raw_cid and rho_cid"
FROM=$(echo "$EXEC" | jq -r '.receipts.transition.body.preimage_raw_cid // empty')
TO=$(echo "$EXEC" | jq -r '.receipts.transition.body.rho_cid // empty')
if [ -n "$FROM" ] && [ -n "$TO" ] && echo "$FROM" | grep -q '^b3:' && echo "$TO" | grep -q '^b3:'; then
  ok "transition.body has b3: CIDs"
else
  fail "transition.body has b3: CIDs (from=$FROM to=$TO)"
fi

# 4) WF parents chain = [wa.body_cid, transition.body_cid]
echo "[4/8] WF parents chain is correct"
WA_CID=$(echo "$EXEC" | jq -r '.receipts.wa.body_cid')
TR_CID=$(echo "$EXEC" | jq -r '.receipts.transition.body_cid')
WF_P0=$(echo "$EXEC" | jq -r '.receipts.wf.parents[0]')
WF_P1=$(echo "$EXEC" | jq -r '.receipts.wf.parents[1]')
if [ "$WF_P0" = "$WA_CID" ] && [ "$WF_P1" = "$TR_CID" ]; then
  ok "wf.parents == [wa.body_cid, transition.body_cid]"
else
  fail "wf.parents mismatch (p0=$WF_P0 wa=$WA_CID p1=$WF_P1 tr=$TR_CID)"
fi

# 5) WF decision is ALLOW
echo "[5/8] WF decision is ALLOW"
DECISION=$(echo "$EXEC" | jq -r '.receipts.wf.body.decision // empty')
if [ "$DECISION" = "ALLOW" ]; then ok "wf.body.decision == ALLOW"; else fail "wf.body.decision (got $DECISION)"; fi

# 6) Determinism: same input → same tip_cid
echo "[6/8] determinism (same input → same tip_cid)"
TIP_A=$(echo "$EXEC" | jq -r '.tip_cid')
EXEC2=$(curl -fsS "$BASE/v1/execute" -H 'content-type: application/json' -d "$EXEC_REQ" 2>/dev/null || echo '{}')
TIP_B=$(echo "$EXEC2" | jq -r '.tip_cid')
if [ "$TIP_A" = "$TIP_B" ]; then ok "tip_cid is deterministic"; else fail "tip_cid differs ($TIP_A vs $TIP_B)"; fi

# 7) Ingest still works (backward compat)
echo "[7/8] ingest + certify"
ING=$(curl -fsS "$BASE/v1/ingest" -H 'content-type: application/json' -d '{"payload":{"n":42},"certify":true}' 2>/dev/null || echo '{}')
ING_CID=$(echo "$ING" | jq -r '.cid // empty')
ING_DID=$(echo "$ING" | jq -r '.did // empty')
if [ -n "$ING_CID" ] && echo "$ING_DID" | grep -q '^did:cid:'; then
  ok "ingest returns cid + did"
else
  fail "ingest (cid=$ING_CID did=$ING_DID)"
fi

# 8) body_cid is content-only (proof/kid don't affect it)
echo "[8/8] body_cid is content-only (separation)"
WF_BODY_CID=$(echo "$EXEC" | jq -r '.receipts.wf.body_cid')
WF_BODY_CID2=$(echo "$EXEC2" | jq -r '.receipts.wf.body_cid')
if [ "$WF_BODY_CID" = "$WF_BODY_CID2" ]; then
  ok "body_cid stable across runs (content-only)"
else
  fail "body_cid differs ($WF_BODY_CID vs $WF_BODY_CID2)"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
if [ "$FAIL" -eq 0 ]; then echo "ALL OK"; exit 0; else echo "SOME CHECKS FAILED"; exit 1; fi
