#!/usr/bin/env node
// UBL Deploy Webhook — receives GitHub release events, downloads binary,
// installs to bin/, restarts ubl-gate via PM2, archives to MinIO.
// Runs on :3003, managed by PM2.

const http = require("http");
const crypto = require("crypto");
const { execSync, exec } = require("child_process");
const fs = require("fs");
const path = require("path");
const os = require("os");

// ── Config ──────────────────────────────────────────────────────
const PORT = parseInt(process.env.WEBHOOK_PORT || "3003", 10);
const SECRET = process.env.WH_SEC || "";
const INSTALL_DIR = process.env.UBL_INSTALL_DIR || path.join(__dirname, "..", "..", "bin");
const PM2_APP = process.env.PM2_APP_NAME || "ubl-gate";
const MINIO_ALIAS = process.env.MINIO_ALIAS || "lab512";
const MINIO_BUCKET = process.env.MINIO_BUCKET || "ubl-releases";
const COMMIT_FILE = path.join(INSTALL_DIR, ".current-commit");
const ASSET_NAME = "ubl-darwin-arm64.tar.gz";

const log = (msg) => console.log(`[webhook] ${new Date().toISOString()} ${msg}`);

// ── HMAC verification ───────────────────────────────────────────
function verifySignature(payload, signature) {
  if (!SECRET) {
    log("WARNING: WH_SEC not set, skipping HMAC verification");
    return true;
  }
  if (!signature) return false;
  const expected = "sha256=" + crypto.createHmac("sha256", SECRET).update(payload).digest("hex");
  return crypto.timingSafeEqual(Buffer.from(expected), Buffer.from(signature));
}

// ── Deploy logic ────────────────────────────────────────────────
async function deploy(release) {
  const tag = release.tag_name;
  const assets = release.assets || [];
  const asset = assets.find((a) => a.name === ASSET_NAME);

  if (!asset) {
    log(`ERROR: no ${ASSET_NAME} in release ${tag}`);
    return { ok: false, error: "asset_not_found" };
  }

  // Check if already deployed
  try {
    const current = fs.readFileSync(COMMIT_FILE, "utf8").trim();
    if (current === tag) {
      log(`Already at ${tag}, skipping`);
      return { ok: true, skipped: true };
    }
  } catch (_) {}

  log(`Deploying ${tag} from ${asset.browser_download_url}`);

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "ubl-deploy-"));
  const tarball = path.join(tmpDir, ASSET_NAME);

  try {
    // Download
    execSync(`curl -sfL "${asset.browser_download_url}" -o "${tarball}"`, { timeout: 120000 });

    // Extract
    execSync(`tar xzf "${tarball}" -C "${tmpDir}"`, { timeout: 30000 });

    const gateBin = path.join(tmpDir, "ubl_gate");
    if (!fs.existsSync(gateBin)) {
      throw new Error("ubl_gate not found in tarball");
    }

    // Install
    fs.mkdirSync(INSTALL_DIR, { recursive: true });
    execSync(`chmod +x "${gateBin}"`);
    fs.copyFileSync(gateBin, path.join(INSTALL_DIR, "ubl_gate"));

    const cliBin = path.join(tmpDir, "ubl");
    if (fs.existsSync(cliBin)) {
      execSync(`chmod +x "${cliBin}"`);
      fs.copyFileSync(cliBin, path.join(INSTALL_DIR, "ubl"));
    }

    fs.writeFileSync(COMMIT_FILE, tag);
    log(`Installed ${tag} → ${INSTALL_DIR}/ubl_gate`);

    // PM2 restart
    try {
      execSync(`pm2 restart ${PM2_APP}`, { timeout: 10000 });
      log(`PM2 restarted ${PM2_APP}`);
    } catch (e) {
      log(`WARNING: PM2 restart failed: ${e.message}`);
    }

    // Health check
    await new Promise((r) => setTimeout(r, 2000));
    try {
      execSync('curl -sf http://localhost:3000/healthz', { timeout: 5000 });
      log("✓ Health check passed");
    } catch (_) {
      log("✗ Health check failed after deploy");
    }

    // Archive to MinIO (non-blocking, best-effort)
    exec(
      `mc cp "${tarball}" "${MINIO_ALIAS}/${MINIO_BUCKET}/${tag}/${ASSET_NAME}" 2>/dev/null`,
      (err) => {
        if (!err) log(`Archived to MinIO ${MINIO_BUCKET}/${tag}/`);
      }
    );

    return { ok: true, tag, installed: true };
  } catch (e) {
    log(`ERROR: deploy failed: ${e.message}`);
    return { ok: false, error: e.message };
  } finally {
    try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch (_) {}
  }
}

// ── HTTP server ─────────────────────────────────────────────────
const server = http.createServer((req, res) => {
  // Health endpoint
  if (req.method === "GET" && req.url === "/healthz") {
    const current = fs.existsSync(COMMIT_FILE)
      ? fs.readFileSync(COMMIT_FILE, "utf8").trim()
      : "none";
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ ok: true, service: "deploy-webhook", current_version: current }));
    return;
  }

  // Status endpoint
  if (req.method === "GET" && req.url === "/status") {
    const current = fs.existsSync(COMMIT_FILE)
      ? fs.readFileSync(COMMIT_FILE, "utf8").trim()
      : "none";
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({
      service: "deploy-webhook",
      current_version: current,
      install_dir: INSTALL_DIR,
      pm2_app: PM2_APP,
      hmac_configured: !!SECRET,
    }));
    return;
  }

  // Webhook endpoint
  if (req.method === "POST" && (req.url === "/" || req.url === "/webhook")) {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", async () => {
      // Verify HMAC
      const sig = req.headers["x-hub-signature-256"];
      if (!verifySignature(body, sig)) {
        log("REJECTED: invalid HMAC signature");
        res.writeHead(403, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "invalid_signature" }));
        return;
      }

      let payload;
      try {
        payload = JSON.parse(body);
      } catch (_) {
        res.writeHead(400, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "invalid_json" }));
        return;
      }

      // Only handle release events
      const event = req.headers["x-github-event"];
      if (event !== "release") {
        log(`Ignoring event: ${event}`);
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ ok: true, ignored: event }));
        return;
      }

      if (payload.action !== "published") {
        log(`Ignoring release action: ${payload.action}`);
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ ok: true, ignored: payload.action }));
        return;
      }

      log(`Release event: ${payload.release.tag_name}`);
      const result = await deploy(payload.release);
      res.writeHead(result.ok ? 200 : 500, { "Content-Type": "application/json" });
      res.end(JSON.stringify(result));
    });
    return;
  }

  res.writeHead(404, { "Content-Type": "application/json" });
  res.end(JSON.stringify({ error: "not_found" }));
});

server.listen(PORT, "0.0.0.0", () => {
  log(`Listening on :${PORT}`);
  log(`HMAC: ${SECRET ? "configured" : "WARNING: not set"}`);
  log(`Install dir: ${INSTALL_DIR}`);
  log(`PM2 app: ${PM2_APP}`);
});
