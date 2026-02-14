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
      name: "deploy-watcher",
      script: "/Users/danvoulez/fractal-byte/infra/deploy-watcher.sh",
      cwd: "/Users/danvoulez/fractal-byte",
      interpreter: "/bin/bash",
      env: {
        MINIO_ALIAS: "lab512",
        MINIO_BUCKET: "ubl-releases",
        DEPLOY_POLL_INTERVAL: "30",
        UBL_INSTALL_DIR: "/Users/danvoulez/fractal-byte/bin",
        PM2_APP_NAME: "ubl-gate",
      },
      max_restarts: 5,
      restart_delay: 10000,
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
