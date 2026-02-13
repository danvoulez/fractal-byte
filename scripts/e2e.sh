#!/usr/bin/env bash
# UBL Gate — Hardened Smoke Test (12 asserts, <5s)
# Requires: gate running on $BASE (default localhost:3000), jq
# NOTE: gate must be freshly started (clean seen_cids) for idempotency test
set -uo pipefail
BASE=${BASE:-http://localhost:3000}
PASS=0; FAIL=0

ok()   { echo "  ✓ $1"; PASS=$((PASS+1)); }
fail() { echo "  ✗ $1"; FAIL=$((FAIL+1)); }

MANIFEST='{"pipeline":"echo","in_grammar":{"inputs":{"raw_b64":""},"mappings":[{"from":"raw_b64","codec":"base64.decode","to":"raw.bytes"}],"output_from":"raw.bytes"},"out_grammar":{"inputs":{"content":""},"mappings":[],"output_from":"content"},"policy":{"allow":true}}'
EXEC_REQ="{\"manifest\":$MANIFEST,\"vars\":{\"input_data\":\"aGVsbG8=\"}}"

echo "=== UBL Gate Hardened Smoke Test ==="
echo ""

# ── 1) Healthz ────────────────────────────────────────────────────────
echo "[1/12] healthz"
HZ=$(curl -fsS "$BASE/healthz" 2>/dev/null || echo '{}')
if [ "$(echo "$HZ" | jq -r .ok)" = "true" ]; then ok "healthz returns ok:true"; else fail "healthz returns ok:true"; fi

# ── 2) Execute returns receipt chain ──────────────────────────────────
echo "[2/12] execute returns receipt chain"
EXEC=$(curl -fsS "$BASE/v1/execute" -H 'content-type: application/json' -d "$EXEC_REQ" 2>/dev/null || echo '{}')
WA_T=$(echo "$EXEC" | jq -r '.receipts.wa.t // empty')
TR_T=$(echo "$EXEC" | jq -r '.receipts.transition.t // empty')
WF_T=$(echo "$EXEC" | jq -r '.receipts.wf.t // empty')
if [ "$WA_T" = "ubl/wa" ] && [ "$TR_T" = "ubl/transition" ] && [ "$WF_T" = "ubl/wf" ]; then
  ok "execute has receipts.wa, .transition, .wf"
else
  fail "execute has receipts.wa, .transition, .wf (got wa=$WA_T tr=$TR_T wf=$WF_T)"
fi

# ── 3) Transition has from/to CIDs ───────────────────────────────────
echo "[3/12] transition receipt has preimage_raw_cid and rho_cid"
FROM=$(echo "$EXEC" | jq -r '.receipts.transition.body.preimage_raw_cid // empty')
TO=$(echo "$EXEC" | jq -r '.receipts.transition.body.rho_cid // empty')
if [ -n "$FROM" ] && [ -n "$TO" ] && echo "$FROM" | grep -q '^b3:' && echo "$TO" | grep -q '^b3:'; then
  ok "transition.body has b3: CIDs"
else
  fail "transition.body has b3: CIDs (from=$FROM to=$TO)"
fi

# ── 4) WF parents chain = [wa.body_cid, transition.body_cid] ────────
echo "[4/12] WF parents chain is correct"
WA_CID=$(echo "$EXEC" | jq -r '.receipts.wa.body_cid')
TR_CID=$(echo "$EXEC" | jq -r '.receipts.transition.body_cid')
WF_P0=$(echo "$EXEC" | jq -r '.receipts.wf.parents[0]')
WF_P1=$(echo "$EXEC" | jq -r '.receipts.wf.parents[1]')
if [ "$WF_P0" = "$WA_CID" ] && [ "$WF_P1" = "$TR_CID" ]; then
  ok "wf.parents == [wa.body_cid, transition.body_cid]"
else
  fail "wf.parents mismatch (p0=$WF_P0 wa=$WA_CID p1=$WF_P1 tr=$TR_CID)"
fi

# ── 5) WF decision is ALLOW ─────────────────────────────────────────
echo "[5/12] WF decision is ALLOW"
DECISION=$(echo "$EXEC" | jq -r '.decision // empty')
if [ "$DECISION" = "ALLOW" ]; then ok "decision == ALLOW"; else fail "decision (got $DECISION)"; fi

# ── 6) Idempotency: replay same input → 409 CONFLICT ────────────────
echo "[6/12] idempotency (replay → 409)"
REPLAY_STATUS=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/v1/execute" -H 'content-type: application/json' -d "$EXEC_REQ" 2>/dev/null)
if [ "$REPLAY_STATUS" = "409" ]; then
  ok "replay returns 409 CONFLICT"
else
  fail "replay expected 409, got $REPLAY_STATUS"
fi

# ── 7) Different input → different tip_cid (determinism by contrast) ─
echo "[7/12] different input → different tip_cid"
EXEC_B_REQ="{\"manifest\":$MANIFEST,\"vars\":{\"input_data\":\"d29ybGQ=\"}}"
EXEC_B=$(curl -fsS "$BASE/v1/execute" -H 'content-type: application/json' -d "$EXEC_B_REQ" 2>/dev/null || echo '{}')
TIP_A=$(echo "$EXEC" | jq -r '.tip_cid')
TIP_B=$(echo "$EXEC_B" | jq -r '.tip_cid')
if [ "$TIP_A" != "$TIP_B" ] && [ -n "$TIP_B" ] && [ "$TIP_B" != "null" ]; then
  ok "different input → different tip_cid"
else
  fail "expected different tip_cids (A=$TIP_A B=$TIP_B)"
fi

# ── 8) Chaining: second run's WA.parents[0] == first run's tip_cid ──
echo "[8/12] strong chaining (WA.parents[0] == prev_tip)"
CHAIN_P0=$(echo "$EXEC_B" | jq -r '.receipts.wa.parents[0] // empty')
if [ "$CHAIN_P0" = "$TIP_A" ]; then
  ok "WA.parents[0] == prev tip_cid"
else
  fail "chaining (WA.parents[0]=$CHAIN_P0 expected=$TIP_A)"
fi

# ── 9) Policy deny → DENY receipt (200, not 422) ────────────────────
echo "[9/12] policy deny → DENY receipt"
DENY_MANIFEST='{"pipeline":"deny","in_grammar":{"inputs":{"x":""},"mappings":[],"output_from":"x"},"out_grammar":{"inputs":{"y":""},"mappings":[],"output_from":"y"},"policy":{"allow":false}}'
DENY_REQ="{\"manifest\":$DENY_MANIFEST,\"vars\":{\"x\":\"data\"}}"
DENY_RESP=$(curl -s -w '\n%{http_code}' "$BASE/v1/execute" -H 'content-type: application/json' -d "$DENY_REQ" 2>/dev/null)
DENY_CODE=$(echo "$DENY_RESP" | tail -1)
DENY_BODY=$(echo "$DENY_RESP" | sed '$d')
if [ "$DENY_CODE" = "200" ] && [ "$(echo "$DENY_BODY" | jq -r '.decision // empty')" = "DENY" ]; then
  ok "policy deny → 200 + DENY receipt"
else
  fail "policy deny (code=$DENY_CODE decision=$(echo "$DENY_BODY" | jq -r '.decision // empty'))"
fi

# ── 10) Ghost mode: receipts exist, ghost flag set ───────────────────
echo "[10/12] ghost mode"
GHOST_REQ="{\"manifest\":$MANIFEST,\"vars\":{\"input_data\":\"Z2hvc3Q=\"},\"ghost\":true}"
GHOST=$(curl -fsS "$BASE/v1/execute" -H 'content-type: application/json' -d "$GHOST_REQ" 2>/dev/null || echo '{}')
GHOST_FLAG=$(echo "$GHOST" | jq -r '.ghost // empty')
GHOST_OBS=$(echo "$GHOST" | jq -r '.receipts.wa.observability.ghost // empty')
if [ "$GHOST_FLAG" = "true" ] && [ "$GHOST_OBS" = "true" ]; then
  ok "ghost=true → receipts exist with observability.ghost=true"
else
  fail "ghost (flag=$GHOST_FLAG obs=$GHOST_OBS)"
fi

# ── 11) Ingest backward compat ──────────────────────────────────────
echo "[11/12] ingest + certify"
ING=$(curl -fsS "$BASE/v1/ingest" -H 'content-type: application/json' -d '{"payload":{"n":42},"certify":true}' 2>/dev/null || echo '{}')
ING_CID=$(echo "$ING" | jq -r '.cid // empty')
ING_DID=$(echo "$ING" | jq -r '.did // empty')
if [ -n "$ING_CID" ] && echo "$ING_DID" | grep -q '^did:cid:'; then
  ok "ingest returns cid + did"
else
  fail "ingest (cid=$ING_CID did=$ING_DID)"
fi

# ── 12) body_cid is content-only (separation) ───────────────────────
echo "[12/12] body_cid is content-only (separation)"
WF_BODY_CID=$(echo "$EXEC" | jq -r '.receipts.wf.body_cid')
if echo "$WF_BODY_CID" | grep -q '^b3:' && [ ${#WF_BODY_CID} -eq 67 ]; then
  ok "body_cid is b3:hex64 format"
else
  fail "body_cid format ($WF_BODY_CID)"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
if [ "$FAIL" -eq 0 ]; then echo "ALL OK"; exit 0; else echo "SOME CHECKS FAILED"; exit 1; fi
