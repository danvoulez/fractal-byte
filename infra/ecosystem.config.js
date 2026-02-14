// PM2 Ecosystem — UBL Production Stack
// Usage: pm2 start infra/ecosystem.config.js
module.exports = {
  apps: [
    {
      name: "ubl-gate",
      script: "/Users/danvoulez/fractal-byte/bin/ubl_gate",
      cwd: "/Users/danvoulez/fractal-byte",
      interpreter: "none",
      env: {
        REGISTRY_BASE_URL: "http://localhost:3000",
        UBL_BIND: "0.0.0.0:3000",
        UBL_AUTH_DISABLED: "0",
        UBL_DEV_TOKEN: "ubl-dev-token-001",
        RUST_LOG: "info,ubl_gate=debug",
      },
      max_restarts: 10,
      restart_delay: 2000,
      watch: false,
      kill_timeout: 5000,
    },
    {
      name: "deploy-webhook",
      script: "/Users/danvoulez/fractal-byte/infra/webhook/server.js",
      cwd: "/Users/danvoulez/fractal-byte",
      interpreter: "node",
      env: {
        WEBHOOK_PORT: "3003",
        WH_SEC: process.env.WH_SEC || "0db390735593a727a5616ee25d58fd33",
        MINIO_ALIAS: "lab512",
        MINIO_BUCKET: "ubl-releases",
        UBL_INSTALL_DIR: "/Users/danvoulez/fractal-byte/bin",
        PM2_APP_NAME: "ubl-gate",
      },
      max_restarts: 10,
      restart_delay: 3000,
      autorestart: true,
    },
    {
      name: "cloudflared",
      script: "/opt/homebrew/bin/cloudflared",
      args: "tunnel --config /Users/danvoulez/fractal-byte/infra/cloudflared.yml run ubl-tunnel",
      interpreter: "none",
      max_restarts: 10,
      restart_delay: 5000,
      autorestart: true,
    },
    {
      name: "minio",
      script: "/Users/danvoulez/start-minio.sh",
      interpreter: "/bin/bash",
      max_restarts: 5,
      restart_delay: 5000,
      autorestart: true,
    },
  ],
};
