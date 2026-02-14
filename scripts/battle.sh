#!/usr/bin/env bash
set -Eeuo pipefail

# ===========================================
# UBL Battle++ Script — full roadmap torture
# ===========================================

# ---------------- ENV / DEFAULTS ----------------
: "${GATE_URL:?Defina GATE_URL (ex: https://api.ubl.example.com)}"
: "${TENANT_A_ID:?Defina TENANT_A_ID}"
: "${TENANT_A_TOKEN:?Defina TENANT_A_TOKEN}"

READONLY_TOKEN="${READONLY_TOKEN:-$TENANT_A_TOKEN}"
TENANT_B_ID="${TENANT_B_ID:-}"
TENANT_B_TOKEN="${TENANT_B_TOKEN:-}"
CLIENT_ID_A="${CLIENT_ID_A:-battle-client-a}"

CORS_OK_ORIGIN_TENANT_A="${CORS_OK_ORIGIN_TENANT_A:-https://app.acme.com}"
CORS_BAD_ORIGIN_TENANT_A="${CORS_BAD_ORIGIN_TENANT_A:-https://evil.example.com}"

# Endpoints (customizáveis se seu Gate usa paths diferentes)
EP_EXEC="${EP_EXEC:-$GATE_URL/v1/execute}"
EP_INGEST="${EP_INGEST:-$GATE_URL/v1/ingest}"
EP_RECEIPTS="${EP_RECEIPTS:-$GATE_URL/v1/receipts}"
EP_RECEIPT_BY_ID="${EP_RECEIPT_BY_ID:-$GATE_URL/v1/receipt}" # + "/{cid}"
EP_TRANSITIONS="${EP_TRANSITIONS:-$GATE_URL/v1/transition}"   # + "/{cid}"
EP_AUDIT="${EP_AUDIT:-$GATE_URL/v1/audit}"
EP_HEALTH="${EP_HEALTH:-$GATE_URL/healthz}"
EP_METRICS="${EP_METRICS:-$GATE_URL/metrics}" # opcional

# Feature toggles
STRICT="${STRICT:-false}"        # true => qualquer fail aborta
ONLY="${ONLY:-}"                 # ex: "cors,rate,policy"
SKIP_RUST="${SKIP_RUST:-false}"
SKIP_SDKS="${SKIP_SDKS:-false}"
SKIP_CORS="${SKIP_CORS:-false}"
SKIP_ADAPTER="${SKIP_ADAPTER:-false}"
SKIP_AUDIT="${SKIP_AUDIT:-false}"
SKIP_CLI="${SKIP_CLI:-false}"

# SDK paths
SDK_TS_DIR="${SDK_TS_DIR:-sdks/ts}"
SDK_PY_DIR="${SDK_PY_DIR:-sdks/py}"

TMP_DIR="$(mktemp -d -t ubl_battle_XXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

# ---------------- STATE / HELPERS ----------------
FAILS=()
WARNS=()
PASSED=0
TESTS=()

section(){ echo -e "\n\033[1;35m==> $*\033[0m"; }
ok(){ echo -e "  \033[1;32m✔\033[0m $*"; ((PASSED++)) || true; }
warn(){ echo -e "  \033[1;33m⚠\033[0m $*"; WARNS+=("$*"); }
fail(){ echo -e "  \033[1;31m✘\033[0m $*"; FAILS+=("$*"); $STRICT && exit 1 || true; }
need(){ command -v "$1" >/dev/null 2>&1 || fail "Dependência ausente: $1"; }
hdr(){ tr '[:upper:]' '[:lower:]' <<<"$1"; }

http_status(){ awk 'BEGIN{RS="\r\n"} /^HTTP\/.* [0-9]{3}/ {code=$2} END{print code}' "$1" 2>/dev/null || echo "000"; }
hdr_val(){ local hfile="$1" name="$2"; awk -v IGNORECASE=1 -v key="$(hdr "$name"):" 'BEGIN{RS="\r\n"} tolower($0) ~ "^"key { sub(/^[^:]*:[[:space:]]*/, "", $0); print $0; exit }' "$hfile" 2>/dev/null || true; }

canon_json(){ jq -S 'del(.observed_at, .server_time, .debug, .meta?.request_id)' 2>/dev/null || cat; }
jqv(){ jq -er "$1" 2>/dev/null || return 1; }

curl_json() {
  # $1 url, $2 method, $3 token, $4 json-data, $5 extra headers (array)
  local url="$1" method="${2:-GET}" token="${3:-$READONLY_TOKEN}" data="${4:-}"
  shift 4 || true
  local headers=("$@")
  local hfile="$TMP_DIR/hdr_$$.txt" bfile="$TMP_DIR/body_$$.json"
  local extra=()
  [[ -n "$data" ]] && extra=(-d "$data")
  curl -sS -X "$method" \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    "${headers[@]}" \
    "${extra[@]}" \
    -D "$hfile" \
    "$url" -o "$bfile"
  echo "$hfile|$bfile"
}

require_headers(){ local hfile="$1"; shift; for h in "$@"; do local v; v="$(hdr_val "$hfile" "$h" || true)"; [[ -n "$v" ]] || fail "Header ausente: $h"; done; }

want(){ [[ -z "$ONLY" ]] && return 0; IFS=',' read -ra xs <<<"$ONLY"; for x in "${xs[@]}"; do [[ "$x" == "$1" ]] && return 0; done; return 1; }

# Detecta endpoint exec/ingest funcional
pick_exec_url(){
  if [[ "$(curl -sS -o /dev/null -w "%{http_code}" "$EP_EXEC")" =~ ^[45][0-9]{2}$ || "$(curl -sS -o /dev/null -w "%{http_code}" "$EP_EXEC")" =~ ^20[0-9]$ ]]; then
    echo "$EP_EXEC"
  elif [[ "$(curl -sS -o /dev/null -w "%{http_code}" "$EP_INGEST")" =~ ^20[0-9]$ || "$(curl -sS -o /dev/null -w "%{http_code}" "$EP_INGEST")" =~ ^[45][0-9]{2}$ ]]; then
    echo "$EP_INGEST"
  else
    echo ""
  fi
}

# ---------- DEPENDÊNCIAS ----------
section "Checando dependências base"
need jq; need curl
ok "Ferramentas base ok (jq, curl)"

# ---------- HEALTH ----------
section "Healthcheck"
if out=$(curl_json "$EP_HEALTH" "GET" "$READONLY_TOKEN"); then
  IFS='|' read -r hfile bfile <<<"$out"
  code="$(http_status "$hfile")"
  [[ "$code" == "200" ]] && ok "/healthz 200" || fail "/healthz não respondeu 200 (status=$code)"
else
  fail "Falha ao consultar /healthz"
fi

# ---------- WORKSPACE RUST ----------
if want rust && [[ "$SKIP_RUST" != "true" ]]; then
  if command -v cargo >/dev/null 2>&1; then
    section "Rust workspace: build + tests + clippy"
    (cargo build --workspace >/dev/null && ok "cargo build") || fail "cargo build falhou"
    (cargo test  --workspace >/dev/null && ok "cargo test")  || fail "cargo test falhou"
    if cargo clippy --workspace -- -D warnings >/dev/null 2>&1; then
      ok "clippy clean"
    else
      warn "clippy com warnings (não bloqueante)"
    fi
  else
    warn "Rust toolchain não detectado; SKIP_RUST=true recomendado"
  fi
fi

# ---------- SDKS ----------
if want sdks && [[ "$SKIP_SDKS" != "true" ]]; then
  section "SDK TypeScript: build + tests + smoke real"
  if [[ -d "$SDK_TS_DIR" ]] && command -v npm >/dev/null 2>&1 && command -v node >/dev/null 2>&1; then
    pushd "$SDK_TS_DIR" >/dev/null
      (npm ci >/dev/null && ok "npm ci") || warn "npm ci falhou"
      (npm run build >/dev/null && ok "tsc build") || warn "tsc build falhou"
      (npm test >/dev/null && ok "ts tests") || warn "ts tests falharam"
      # Smoke real (usa Gate): gera script JS em tempo real
      cat > "$TMP_DIR/ts_smoke.mjs" <<'JS'
import { UBLClient } from "./dist/index.js";
const url = process.env.GATE_URL;
const token = process.env.TENANT_A_TOKEN;
const c = new UBLClient({ baseURL: url, token });
const payload = { inputs: { smoke: true }, manifest: { kind: "noop" } };
const r = await c.execute(payload).catch(e => ({ error: e?.message, status: e?.status }));
console.log(JSON.stringify(r));
JS
      if GATE_URL="$GATE_URL" TENANT_A_TOKEN="$TENANT_A_TOKEN" node "$TMP_DIR/ts_smoke.mjs" > "$TMP_DIR/ts_smoke.out" 2>/dev/null; then
        grep -q '"cid"\|"wf"' "$TMP_DIR/ts_smoke.out" && ok "SDK TS smoke (execute) OK" || warn "SDK TS smoke não retornou receipt esperado"
      else
        warn "SDK TS smoke falhou"
      fi
    popd >/dev/null
  else
    warn "SDK TS ausente ou npm/node indisponíveis"
  fi

  section "SDK Python: install + tests + smoke real"
  if [[ -d "$SDK_PY_DIR" ]] && command -v python3 >/dev/null 2>&1; then
    pushd "$SDK_PY_DIR" >/dev/null
      (python3 -m pip install -e ".[dev]" >/dev/null && ok "pip install -e .[dev]") || warn "pip install falhou"
      (python3 -m pytest -q >/dev/null && ok "pytest (Py SDK)") || warn "pytest falhou"
      cat > "$TMP_DIR/py_smoke.py" <<'PY'
import os, json
from ubl_sdk import UBLClient
c = UBLClient(base_url=os.environ["GATE_URL"], token=os.environ["TENANT_A_TOKEN"])
try:
  r = c.execute({"inputs":{"smoke":True},"manifest":{"kind":"noop"}})
  print(json.dumps({"ok":True,"cid":r.get("cid") or r.get("wf",{}).get("cid")}))
except Exception as e:
  print(json.dumps({"ok":False,"err":str(e)}))
PY
      if GATE_URL="$GATE_URL" TENANT_A_TOKEN="$TENANT_A_TOKEN" python3 "$TMP_DIR/py_smoke.py" > "$TMP_DIR/py_smoke.out" 2>/dev/null; then
        grep -q '"ok": true' "$TMP_DIR/py_smoke.out" && ok "SDK Py smoke (execute) OK" || warn "SDK Py smoke não retornou ok"
      else
        warn "SDK Py smoke falhou"
      fi
    popd >/dev/null
  else
    warn "SDK Py ausente ou python3 indisponível"
  fi
fi

# ---------- EXEC / DETERMINISM / CHAIN ----------
if want exec; then
  section "Determinismo RB-VM e Chain sanity"
  EXEC_URL="$(pick_exec_url)"
  if [[ -z "$EXEC_URL" ]]; then
    warn "Não encontrei endpoint EXEC/INGEST acessível; pulando bloco"
  else
    payload='{"inputs":{"x":1},"manifest":{"kind":"noop"}}'
    CIDS=()
    for i in {1..5}; do
      out=$(curl_json "$EXEC_URL" POST "$TENANT_A_TOKEN" "$payload")
      IFS='|' read -r hfile bfile <<<"$out"
      code="$(http_status "$hfile")"
      [[ "$code" =~ ^20[01]$ ]] || fail "Execução $i falhou (status=$code) body=$(cat "$bfile")"
      cid="$(jq -er '.receipt.cid // .wf.cid // .cid' "$bfile" 2>/dev/null || true)"
      [[ -n "$cid" ]] || fail "Sem CID na execução $i"
      CIDS+=("$cid")
    done
    if printf "%s\n" "${CIDS[@]}" | awk 'NR==1{s=$0} $0!=s{d=1} END{exit d?0:1}'; then
      fail "CIDs divergiram: ${CIDS[*]}"
    else
      ok "Mesmos CIDs em 5 execuções: ${CIDS[0]}"
    fi

    # Chain sanity (WA, Transition, WF)
    out=$(curl_json "$EXEC_URL" POST "$TENANT_A_TOKEN" '{"inputs":{"sanity":true},"manifest":{"kind":"noop"}}')
    IFS='|' read -r hfile bfile <<<"$out"
    wa="$(jq -er '.wa.cid' "$bfile" 2>/dev/null || true)"
    trn="$(jq -er '.transition.cid' "$bfile" 2>/dev/null || true)"
    wf="$(jq -er '.wf.cid // .receipt.cid // .cid' "$bfile" 2>/dev/null || true)"
    [[ -n "$wa" && -n "$trn" && -n "$wf" ]] && ok "Chain: WA=$wa Transition=$trn WF=$wf" || fail "Chain incompleta"

    # GET por CID (receipt e transition)
    if [[ -n "$wf" ]]; then
      out=$(curl_json "$EP_RECEIPT_BY_ID/$wf" GET "$READONLY_TOKEN")
      IFS='|' read -r hfile bfile <<<"$out"; [[ "$(http_status "$hfile")" =~ ^20[0-9]$ ]] && ok "GET receipt/$wf" || fail "GET receipt/$wf falhou"
      out=$(curl_json "$EP_TRANSITIONS/$trn" GET "$READONLY_TOKEN")
      IFS='|' read -r hfile bfile <<<"$out"; [[ "$(http_status "$hfile")" =~ ^20[0-9]$ ]] && ok "GET transition/$trn" || warn "GET transition/$trn indisponível"
    fi
  fi
fi

# ---------- POLICY (trace / deny / warn) ----------
if want policy; then
  section "Política em cascata (policy_trace / deny / warn)"
  EXEC_URL="$(pick_exec_url)"
  if [[ -z "$EXEC_URL" ]]; then
    warn "Sem EXEC_URL; pulando política"
  else
    # PASS case
    out=$(curl_json "$EXEC_URL" POST "$TENANT_A_TOKEN" '{"inputs":{"country":"EU"},"manifest":{"kind":"policy-check"}}')
    IFS='|' read -r hfile bfile <<<"$out"
    trace_len="$(jq -er '.wf.body.policy_trace | length' "$bfile" 2>/dev/null || echo 0)"
    [[ "$trace_len" -gt 0 ]] && ok "policy_trace presente (len=$trace_len)" || warn "policy_trace ausente (regras vazias?)"
    # Invariantes mínimos
    lvl="$(jq -er '.wf.body.policy_trace[0].level' "$bfile" 2>/dev/null || true)"
    act="$(jq -er '.wf.body.policy_trace[0].action' "$bfile" 2>/dev/null || true)"
    [[ -n "$lvl" && -n "$act" ]] && ok "policy_trace[0]: level=$lvl action=$act" || warn "policy_trace sem level/action"

    # Tenta provocar DENY (heurística)
    out=$(curl_json "$EXEC_URL" POST "$TENANT_A_TOKEN" '{"inputs":{"force_deny":true},"manifest":{"kind":"policy-check"}}')
    IFS='|' read -r hfile bfile <<<"$out"
    decision="$(jq -er '.wf.body.decision // .receipt.decision // .decision' "$bfile" 2>/dev/null || echo "")"
    if [[ "$decision" == "DENY" ]]; then
      ok "DENY observado por política"
    else
      warn "Não consegui provocar DENY (inputs/rules podem ser diferentes no ambiente)"
    fi

    # Tenta provocar WARN (continua execução)
    out=$(curl_json "$EXEC_URL" POST "$TENANT_A_TOKEN" '{"inputs":{"maybe_warn":true},"manifest":{"kind":"policy-check"}}')
    IFS='|' read -r hfile bfile <<<"$out"
    has_warn="$(jq -er '[.wf.body.policy_trace[]? | select(.action=="WARN")] | length' "$bfile" 2>/dev/null || echo 0)"
    [[ "$has_warn" -gt 0 ]] && ok "WARN registrado em policy_trace" || warn "Sem WARN detectável (ok se não aplicável)"
  fi
fi

# ---------- RATE LIMIT ----------
if want rate; then
  section "Rate limiting por X-Client-Id ($CLIENT_ID_A)"
  saw429=false
  for i in {1..250}; do
    curl -sS -o /dev/null -D "$TMP_DIR/rl_hdr.txt" \
      -H "X-Client-Id: $CLIENT_ID_A" \
      -H "Authorization: Bearer $TENANT_A_TOKEN" \
      "$EP_HEALTH" || true
    code="$(http_status "$TMP_DIR/rl_hdr.txt")"
    if [[ "$code" == "429" ]]; then
      saw429=true; break
    fi
  done
  if $saw429; then
    require_headers "$TMP_DIR/rl_hdr.txt" "Retry-After" "RateLimit-Limit" "RateLimit-Remaining"
    ok "429 observado com cabeçalhos de rate limit"
  else
    warn "Não observei 429 (limite alto? ambiente dev)"
  fi
fi

# ---------- CORS POR TENANT ----------
if want cors && [[ "$SKIP_CORS" != "true" ]]; then
  section "CORS (preflight, vary, deny ruim, cross-tenant)"
  # Preflight permitido
  curl -sS -X OPTIONS -D "$TMP_DIR/cors_ok_hdr.txt" \
    -H "Origin: $CORS_OK_ORIGIN_TENANT_A" \
    -H "Access-Control-Request-Method: POST" \
    -H "Access-Control-Request-Headers: authorization, content-type" \
    "$EP_RECEIPTS" -o /dev/null || true
  code="$(http_status "$TMP_DIR/cors_ok_hdr.txt")"
  acao="$(hdr_val "$TMP_DIR/cors_ok_hdr.txt" "Access-Control-Allow-Origin" || true)"
  vary="$(hdr_val "$TMP_DIR/cors_ok_hdr.txt" "Vary" || true)"
  if [[ "$code" =~ ^20[0-9]$ && "$acao" == "$CORS_OK_ORIGIN_TENANT_A" ]]; then
    ok "Preflight permitido p/ origem boa"
  else
    fail "Preflight não refletiu origem permitida (status=$code, ACAO='$acao')"
  fi
  [[ "$vary" =~ [Oo]rigin ]] && ok "Vary inclui Origin" || fail "Vary sem Origin"

  # Preflight negado
  curl -sS -X OPTIONS -D "$TMP_DIR/cors_bad_hdr.txt" \
    -H "Origin: $CORS_BAD_ORIGIN_TENANT_A" \
    -H "Access-Control-Request-Method: GET" \
    "$EP_RECEIPTS" -o /dev/null || true
  acao_bad="$(hdr_val "$TMP_DIR/cors_bad_hdr.txt" "Access-Control-Allow-Origin" || true)"
  [[ -z "$acao_bad" ]] && ok "Origem ruim não refletida (negada)" || warn "ACAO presente para origem ruim"

  # Cross-tenant: se TENANT_B existir, testamos que a origem de A não é automaticamente válida em B
  if [[ -n "$TENANT_B_ID" && -n "$TENANT_B_TOKEN" ]]; then
    curl -sS -X OPTIONS -D "$TMP_DIR/cors_cross_hdr.txt" \
      -H "Origin: $CORS_OK_ORIGIN_TENANT_A" \
      -H "Access-Control-Request-Method: GET" \
      "$EP_RECEIPTS" -H "Authorization: Bearer $TENANT_B_TOKEN" -o /dev/null || true
    acao_cross="$(hdr_val "$TMP_DIR/cors_cross_hdr.txt" "Access-Control-Allow-Origin" || true)"
    [[ -z "$acao_cross" || "$acao_cross" != "$CORS_OK_ORIGIN_TENANT_A" ]] && ok "Cross-tenant não vaza allowlist" || warn "Cross-tenant liberou origem de A para B"
  fi
fi

# ---------- SECURITY: 401 / 403 ----------
if want auth; then
  section "Security: 401 (token inválido) e 403 (escopo)"
  # 401: token inválido
  out=$(curl_json "$EP_RECEIPTS" GET "bad.token.value")
  IFS='|' read -r hfile bfile <<<"$out"
  [[ "$(http_status "$hfile")" == "401" ]] && ok "401 com token inválido" || warn "Não obtive 401 com token inválido"

  # 403: se houver multi-tenant, tenta usar token B acessando recurso A (heurística)
  if [[ -n "$TENANT_B_TOKEN" ]]; then
    EXEC_URL="$(pick_exec_url)"
    if [[ -n "$EXEC_URL" ]]; then
      out=$(curl_json "$EXEC_URL" POST "$TENANT_A_TOKEN" '{"inputs":{"owner":"A"},"manifest":{"kind":"noop"}}')
      IFS='|' read -r h2 b2 <<<"$out"
      cidA="$(jq -er '.wf.cid // .receipt.cid // .cid' "$b2" 2>/dev/null || true)"
      if [[ -n "$cidA" ]]; then
        out=$(curl_json "$EP_RECEIPT_BY_ID/$cidA" GET "$TENANT_B_TOKEN")
        IFS='|' read -r h3 b3 <<<"$out"
        code="$(http_status "$h3")"
        [[ "$code" == "403" || "$code" == "404" ]] && ok "Acesso cruzado bloqueado ($code)" || warn "Acesso cruzado não bloqueado (status=$code)"
      fi
    fi
  fi
fi

# ---------- ADAPTERS HTTP / CID ----------
if want adapter && [[ "$SKIP_ADAPTER" != "true" ]]; then
  section "Adapters HTTP: pinagem CID (sanity)"
  EXEC_URL="$(pick_exec_url)"
  if [[ -z "$EXEC_URL" ]]; then
    warn "Sem EXEC_URL; pulando adapters"
  else
    out=$(curl_json "$EXEC_URL" POST "$TENANT_A_TOKEN" '{"inputs":{"fetch":"https://example.com"},"manifest":{"kind":"http-adapter"}}')
    IFS='|' read -r hfile bfile <<<"$out"
    cid="$(jq -er '.wf.body.http.body_cid // .wf.body.pinned.cid // .wf.body_cid' "$bfile" 2>/dev/null || true)"
    [[ -n "$cid" ]] && ok "Resposta traz CID fixado ($cid)" || warn "Não encontrei campo de pinagem de CID"
    [[ "${cid:-}" =~ ^b3: ]] && ok "CID tem prefixo b3:" || warn "CID sem prefixo b3: (ok se outro formato)"
  fi
fi

# ---------- AUDIT (resumo, filtros, integridade) ----------
if want audit && [[ "$SKIP_AUDIT" != "true" ]]; then
  section "Auditoria /v1/audit (resumo e filtros)"
  if [[ "$(curl -sS -o /dev/null -w "%{http_code}" -H "Authorization: Bearer $READONLY_TOKEN" "$EP_AUDIT")" =~ ^20[0-9]$ ]]; then
    out=$(curl_json "$EP_AUDIT" GET "$READONLY_TOKEN")
    IFS='|' read -r hfile bfile <<<"$out"
    total="$(jq -er '.summary.total_receipts' "$bfile" 2>/dev/null || echo 0)"
    ok "Audit disponível (total_receipts=$total)"
    # filtros simples (se suportado): ?decision=DENY
    if [[ "$(curl -sS -o /dev/null -w "%{http_code}" -H "Authorization: Bearer $READONLY_TOKEN" "$EP_AUDIT?decision=DENY")" =~ ^20[0-9]$ ]]; then
      out=$(curl_json "$EP_AUDIT?decision=DENY" GET "$READONLY_TOKEN")
      IFS='|' read -r h2 b2 <<<"$out"
      jq -e '.summary' "$b2" >/dev/null 2>&1 && ok "Filtro decision=DENY responde" || warn "Filtro decision=DENY não estruturado"
    fi
  else
    warn "Endpoint /v1/audit indisponível"
  fi
fi

# ---------- REGISTRY LIST/PAGING ----------
if want registry; then
  section "Registry /v1/receipts (list + paginação)"
  out=$(curl_json "$EP_RECEIPTS?limit=5" GET "$READONLY_TOKEN")
  IFS='|' read -r hfile bfile <<<"$out"
  if [[ "$(http_status "$hfile")" =~ ^20[0-9]$ ]]; then
    count="$(jq -er 'length' "$bfile" 2>/dev/null || echo 0)"
    ok "Listagem ok (itens=$count)"
  else
    warn "Listagem indisponível"
  fi
fi

# ---------- EDGE PROTECTIONS ----------
if want edge; then
  section "Proteções de borda (413/415 + headers padrões)"
  # 413
  dd if=/dev/zero bs=1048576 count=2 2>/dev/null | \
  curl -sS -X POST "$EP_EXEC" \
    -H "Authorization: Bearer $TENANT_A_TOKEN" \
    -H "Content-Type: application/json" \
    --data-binary @- -o /dev/null -D "$TMP_DIR/edge_hdr_413.txt" || true
  code="$(http_status "$TMP_DIR/edge_hdr_413.txt")"
  [[ "$code" == "413" ]] && ok "Payload >1MiB → 413" || warn "Não observei 413 (limite diferente?)"

  # 415
  echo "not-json" | \
  curl -sS -X POST "$EP_EXEC" \
    -H "Authorization: Bearer $TENANT_A_TOKEN" \
    -H "Content-Type: text/plain" \
    --data-binary @- -o /dev/null -D "$TMP_DIR/edge_hdr_415.txt" || true
  code="$(http_status "$TMP_DIR/edge_hdr_415.txt")"
  [[ "$code" == "415" ]] && ok "Content-Type inválido → 415" || warn "Não observei 415 (config liberal?)"
fi

# ---------- CLI ublx ----------
if want cli && [[ "$SKIP_CLI" != "true" ]]; then
  section "CLI ublx (health, cid, opcional execute)"
  if command -v ublx >/dev/null 2>&1; then
    (UBL_GATE_URL="$GATE_URL" UBL_TOKEN="$TENANT_A_TOKEN" ublx health >/dev/null && ok "ublx health") || warn "ublx health falhou"
    printf 'hello' > "$TMP_DIR/cid_test.txt"
    (ublx cid "$TMP_DIR/cid_test.txt" >/dev/null && ok "ublx cid") || warn "ublx cid falhou"
    # opcional execute (se comando existir no seu CLI)
    if ublx help 2>&1 | grep -qi execute; then
      echo '{"inputs":{"cli":true},"manifest":{"kind":"noop"}}' > "$TMP_DIR/cli_exec.json"
      (UBL_GATE_URL="$GATE_URL" UBL_TOKEN="$TENANT_A_TOKEN" ublx execute "$TMP_DIR/cli_exec.json" >/dev/null && ok "ublx execute") || warn "ublx execute falhou"
    fi
  else
    warn "ublx não encontrado no PATH"
  fi
fi

# ---------- MÉTRICAS ----------
if want metrics; then
  section "Metrics (opcional)"
  if [[ "$(curl -sS -o /dev/null -w "%{http_code}" "$EP_METRICS")" == "200" ]]; then
    ok "/metrics 200 (Prometheus)"
  else
    warn "/metrics indisponível (ok em prod fechado)"
  fi
fi

# ---------- RESUMO ----------
section "Resumo"
echo "  Passed: $PASSED"
[[ "${#WARNS[@]}" -gt 0 ]] && { echo "  Warnings: ${#WARNS[@]}"; printf '   - %s\n' "${WARNS[@]}"; }
[[ "${#FAILS[@]}" -gt 0 ]] && { echo "  Fails: ${#FAILS[@]}"; printf '   - %s\n' "${FAILS[@]}"; $STRICT && exit 1 || exit 0; }
ok "BATTLE++ COMPLETA — máxima cobertura possível no ambiente atual."
exit 0
