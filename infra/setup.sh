#!/usr/bin/env bash
# UBL Infrastructure Setup — One-shot bootstrap
# Usage: bash infra/setup.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ENV_FILE="${SCRIPT_DIR}/.env.infra"

echo "═══ UBL Infrastructure Setup ═══"
echo ""

# ── 0. Check prerequisites ───────────────────────────────────────
echo "── Checking prerequisites ──"
for cmd in pm2 cloudflared minio mc cargo curl; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "  ✗ $cmd not found — install it first"
        exit 1
    fi
    echo "  ✓ $cmd"
done

# ── 1. Load env ──────────────────────────────────────────────────
if [ -f "$ENV_FILE" ]; then
    echo ""
    echo "── Loading ${ENV_FILE} ──"
    set -a; source "$ENV_FILE"; set +a
    echo "  ✓ Loaded"
else
    echo ""
    echo "  ⚠ ${ENV_FILE} not found"
    echo "  Copy infra/.env.infra.example → infra/.env.infra and fill in credentials"
    echo "  Then re-run this script."
    exit 1
fi

# ── 2. MinIO bucket ──────────────────────────────────────────────
echo ""
echo "── MinIO: ensuring ubl-releases bucket ──"
mc alias set "${MINIO_ALIAS}" "${MINIO_ENDPOINT}" "${MINIO_ACCESS_KEY}" "${MINIO_SECRET_KEY}" --quiet
if mc ls "${MINIO_ALIAS}/${MINIO_BUCKET}/" &>/dev/null; then
    echo "  ✓ Bucket ${MINIO_BUCKET} exists"
else
    mc mb "${MINIO_ALIAS}/${MINIO_BUCKET}"
    echo "  ✓ Created ${MINIO_BUCKET}"
fi
mc version enable "${MINIO_ALIAS}/${MINIO_BUCKET}" 2>/dev/null || true
echo "  ✓ Versioning enabled"

# ── 3. Build local binary (first deploy) ─────────────────────────
echo ""
echo "── Building release binary ──"
mkdir -p "${UBL_INSTALL_DIR}"
(cd "$PROJECT_DIR" && cargo build --release -p ubl_gate -p ubl)
cp "${PROJECT_DIR}/target/release/ubl_gate" "${UBL_INSTALL_DIR}/ubl_gate"
cp "${PROJECT_DIR}/target/release/ubl" "${UBL_INSTALL_DIR}/ubl"
chmod +x "${UBL_INSTALL_DIR}/ubl_gate" "${UBL_INSTALL_DIR}/ubl"
echo "  ✓ Installed to ${UBL_INSTALL_DIR}"

# ── 4. Cloudflare tunnel ─────────────────────────────────────────
echo ""
echo "── Cloudflare tunnel ──"
if cloudflared tunnel list 2>/dev/null | grep -q "${CF_TUNNEL_NAME}"; then
    echo "  ✓ Tunnel '${CF_TUNNEL_NAME}' already exists"
else
    echo "  Creating tunnel '${CF_TUNNEL_NAME}'..."
    cloudflared tunnel create "${CF_TUNNEL_NAME}"
    echo "  ✓ Tunnel created"
fi

# Route DNS (idempotent)
echo "  Routing DNS: ${CF_DOMAIN} → ${CF_TUNNEL_NAME}"
cloudflared tunnel route dns "${CF_TUNNEL_NAME}" "${CF_DOMAIN}" 2>/dev/null || echo "  (DNS route may already exist)"
echo "  ✓ DNS configured"

# ── 5. GitHub Actions secrets ─────────────────────────────────────
echo ""
echo "── GitHub Actions secrets ──"
if [ -n "${GITHUB_TOKEN:-}" ] && command -v gh &>/dev/null; then
    echo "  Setting MINIO_ENDPOINT, MINIO_ACCESS_KEY, MINIO_SECRET_KEY..."
    echo "${MINIO_ENDPOINT}" | gh secret set MINIO_ENDPOINT -R "${GITHUB_REPO}" 2>/dev/null || echo "  (set MINIO_ENDPOINT manually)"
    echo "${MINIO_ACCESS_KEY}" | gh secret set MINIO_ACCESS_KEY -R "${GITHUB_REPO}" 2>/dev/null || echo "  (set MINIO_ACCESS_KEY manually)"
    echo "${MINIO_SECRET_KEY}" | gh secret set MINIO_SECRET_KEY -R "${GITHUB_REPO}" 2>/dev/null || echo "  (set MINIO_SECRET_KEY manually)"
    echo "  ✓ Secrets set"
else
    echo "  ⚠ gh CLI not found or GITHUB_TOKEN not set"
    echo "  Set these secrets manually in GitHub → Settings → Secrets → Actions:"
    echo "    MINIO_ENDPOINT=${MINIO_ENDPOINT}"
    echo "    MINIO_ACCESS_KEY=${MINIO_ACCESS_KEY}"
    echo "    MINIO_SECRET_KEY=<your secret key>"
fi

# ── 6. PM2 ecosystem ─────────────────────────────────────────────
echo ""
echo "── PM2: starting ecosystem ──"
# Stop old processes if they exist
pm2 delete ubl-gate deploy-watcher cloudflared 2>/dev/null || true
pm2 start "${SCRIPT_DIR}/ecosystem.config.js"
pm2 save
echo "  ✓ PM2 ecosystem started"

# ── 7. Health check ──────────────────────────────────────────────
echo ""
echo "── Health check ──"
sleep 3
if curl -sf http://localhost:3000/healthz >/dev/null 2>&1; then
    echo "  ✓ ubl_gate is healthy on :3000"
else
    echo "  ⚠ ubl_gate not responding yet (may need a few more seconds)"
fi

echo ""
echo "═══ Setup Complete ═══"
echo ""
echo "Services:"
pm2 list
echo ""
echo "Endpoints:"
echo "  Local:  http://localhost:3000/healthz"
echo "  Public: https://${CF_DOMAIN}/healthz (once tunnel is active)"
echo ""
echo "Next steps:"
echo "  1. Verify: curl https://${CF_DOMAIN}/healthz"
echo "  2. Push to main → GitHub Actions builds → MinIO → deploy-watcher auto-deploys"
echo "  3. Monitor: pm2 logs | pm2 monit"
