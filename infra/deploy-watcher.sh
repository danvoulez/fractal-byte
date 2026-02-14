#!/usr/bin/env bash
# UBL Deploy Watcher
# Polls GitHub Releases for new binary versions, downloads, archives to
# local MinIO, and hot-swaps ubl_gate via PM2.
# Designed to run as a PM2 managed process.
set -euo pipefail

# ── Config (from env or defaults) ────────────────────────────────────
GITHUB_REPO="${GITHUB_REPO:-danvoulez/fractal-byte}"
MINIO_ALIAS="${MINIO_ALIAS:-lab512}"
MINIO_BUCKET="${MINIO_BUCKET:-ubl-releases}"
POLL_INTERVAL="${DEPLOY_POLL_INTERVAL:-30}"
INSTALL_DIR="${UBL_INSTALL_DIR:-/Users/danvoulez/fractal-byte/bin}"
COMMIT_FILE="${INSTALL_DIR}/.current-commit"
PM2_APP_NAME="${PM2_APP_NAME:-ubl-gate}"
LOG_PREFIX="[deploy-watcher]"

mkdir -p "$INSTALL_DIR"

echo "$LOG_PREFIX Starting (poll every ${POLL_INTERVAL}s, repo=${GITHUB_REPO})"

while true; do
    # 1. Get latest release tag from GitHub API
    RELEASE_JSON=$(curl -sf "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" 2>/dev/null || echo "")

    if [ -z "$RELEASE_JSON" ]; then
        sleep "$POLL_INTERVAL"
        continue
    fi

    REMOTE_TAG=$(echo "$RELEASE_JSON" | grep -o '"tag_name":"[^"]*"' | head -1 | cut -d'"' -f4)

    if [ -z "$REMOTE_TAG" ]; then
        sleep "$POLL_INTERVAL"
        continue
    fi

    # 2. Compare with local
    LOCAL_TAG=$(cat "$COMMIT_FILE" 2>/dev/null || echo "")

    if [ "$REMOTE_TAG" = "$LOCAL_TAG" ]; then
        sleep "$POLL_INTERVAL"
        continue
    fi

    echo "$LOG_PREFIX New release detected: ${REMOTE_TAG} (was: ${LOCAL_TAG:-none})"

    # 3. Find download URL for the tarball asset
    DOWNLOAD_URL=$(echo "$RELEASE_JSON" | grep -o '"browser_download_url":"[^"]*ubl-darwin-arm64.tar.gz"' | head -1 | cut -d'"' -f4)

    if [ -z "$DOWNLOAD_URL" ]; then
        echo "$LOG_PREFIX ERROR: no ubl-darwin-arm64.tar.gz asset in release ${REMOTE_TAG}"
        sleep "$POLL_INTERVAL"
        continue
    fi

    # 4. Download tarball
    TMP_DIR=$(mktemp -d)
    echo "$LOG_PREFIX Downloading ${DOWNLOAD_URL}"
    if ! curl -sfL "$DOWNLOAD_URL" -o "${TMP_DIR}/ubl-darwin-arm64.tar.gz"; then
        echo "$LOG_PREFIX ERROR: download failed"
        rm -rf "$TMP_DIR"
        sleep "$POLL_INTERVAL"
        continue
    fi

    # 5. Extract and validate
    tar xzf "${TMP_DIR}/ubl-darwin-arm64.tar.gz" -C "$TMP_DIR"

    if [ ! -f "${TMP_DIR}/ubl_gate" ]; then
        echo "$LOG_PREFIX ERROR: ubl_gate binary not found in tarball"
        rm -rf "$TMP_DIR"
        sleep "$POLL_INTERVAL"
        continue
    fi

    chmod +x "${TMP_DIR}/ubl_gate"
    [ -f "${TMP_DIR}/ubl" ] && chmod +x "${TMP_DIR}/ubl"

    # 6. Atomic swap
    mv "${TMP_DIR}/ubl_gate" "${INSTALL_DIR}/ubl_gate"
    [ -f "${TMP_DIR}/ubl" ] && mv "${TMP_DIR}/ubl" "${INSTALL_DIR}/ubl"
    echo "$REMOTE_TAG" > "$COMMIT_FILE"
    echo "$LOG_PREFIX Installed ${REMOTE_TAG} → ${INSTALL_DIR}/ubl_gate"

    # 7. Archive to local MinIO (versioned)
    if command -v mc &>/dev/null; then
        SHORT=$(echo "$REMOTE_TAG" | sed 's/^v0\.1\.0-//')
        mc cp "${TMP_DIR}/../ubl-darwin-arm64.tar.gz" "${MINIO_ALIAS}/${MINIO_BUCKET}/${SHORT}/ubl-darwin-arm64.tar.gz" 2>/dev/null \
            && echo "$LOG_PREFIX Archived to MinIO ${MINIO_BUCKET}/${SHORT}/" \
            || echo "$LOG_PREFIX WARNING: MinIO archive failed (non-fatal)"
    fi

    rm -rf "$TMP_DIR"

    # 8. Restart via PM2
    if pm2 describe "$PM2_APP_NAME" > /dev/null 2>&1; then
        pm2 restart "$PM2_APP_NAME"
        echo "$LOG_PREFIX PM2 restarted ${PM2_APP_NAME}"
    else
        echo "$LOG_PREFIX WARNING: PM2 app '${PM2_APP_NAME}' not found, skipping restart"
    fi

    # 9. Health check (wait 3s then probe)
    sleep 3
    if curl -sf http://localhost:3000/healthz > /dev/null 2>&1; then
        echo "$LOG_PREFIX ✓ Health check passed after deploy"
    else
        echo "$LOG_PREFIX ✗ Health check FAILED after deploy — check pm2 logs ubl-gate"
    fi

    sleep "$POLL_INTERVAL"
done
