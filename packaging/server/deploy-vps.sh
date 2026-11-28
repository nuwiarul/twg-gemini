#!/usr/bin/env bash
set -euo pipefail

# Helper script to install or update twg-server on an Ubuntu/Debian VPS.
# Usage (run on the VPS inside the cloned/extracted repo directory):
#   sudo bash packaging/server/deploy-vps.sh [--skip-build] [--with-assets]

INSTALL_DIR="/opt/twg-server"
SERVICE_USER="twg"
SKIP_BUILD=0
WITH_ASSETS=0

for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=1 ;;
    --with-assets) WITH_ASSETS=1 ;;
    *) echo "Unknown flag: $arg" >&2; exit 1 ;;
  esac
done

if [[ $EUID -ne 0 ]]; then
  echo "Please run as root (sudo bash packaging/server/deploy-vps.sh ...)" >&2
  exit 1
fi

if ! id "$SERVICE_USER" >/dev/null 2>&1; then
  useradd --system --create-home --shell /bin/bash "$SERVICE_USER"
fi

mkdir -p "$INSTALL_DIR/scripts" "$INSTALL_DIR/assets/uploads" "$INSTALL_DIR/assets/models" "$INSTALL_DIR/assets/media" "$INSTALL_DIR/assets/catalog"

if [[ $SKIP_BUILD -eq 0 ]]; then
  echo "==> Building twg-server (release)..."
  cargo build --release -p twg-server
fi

if [[ ! -f target/release/twg-server ]]; then
  echo "Binary target/release/twg-server not found!" >&2
  exit 1
fi

echo "==> Stopping existing service (if running)..."
systemctl stop twg-server 2>/dev/null || true

echo "==> Installing binary and conversion scripts..."
install -m 0755 target/release/twg-server "$INSTALL_DIR/twg-server"
install -m 0644 scripts/gltf-pipeline.mjs "$INSTALL_DIR/scripts/gltf-pipeline.mjs"
install -m 0755 scripts/convert-single-model.mjs "$INSTALL_DIR/scripts/convert-single-model.mjs"
install -m 0644 packaging/server/compose.prod.yaml "$INSTALL_DIR/compose.yaml"

if [[ $WITH_ASSETS -eq 1 && -d assets/models ]]; then
  echo "==> Syncing converted 3D assets (models + catalog)..."
  rsync -av --delete assets/models/ "$INSTALL_DIR/assets/models/"
  mkdir -p "$INSTALL_DIR/assets/catalog"
  cp -f assets/catalog/models.json "$INSTALL_DIR/assets/catalog/models.json"
fi

if [[ ! -f "$INSTALL_DIR/.env" ]]; then
  echo "==> Creating initial .env with generated PASETO_SYMMETRIC_KEY..."
  PASETO_KEY="$(openssl rand -hex 32)"
  cat <<EOF > "$INSTALL_DIR/.env"
POSTGRES_DB=twg_rust
POSTGRES_USER=twg
POSTGRES_PASSWORD=CHANGE_ME_STRONG_DB_PASSWORD
TWG_DB_PORT=5435

DATABASE_URL=postgres://twg:CHANGE_ME_STRONG_DB_PASSWORD@127.0.0.1:5435/twg_rust
SERVER_BIND_ADDR=127.0.0.1:8010
PASETO_SYMMETRIC_KEY=${PASETO_KEY}
ACCESS_TOKEN_TTL_SECS=86400
REFRESH_TOKEN_TTL_SECS=604800
UPLOAD_DIR=${INSTALL_DIR}/assets/uploads
ASSETS_DIR=${INSTALL_DIR}/assets
CORS_ALLOWED_ORIGINS=*
RUST_LOG=info,twg_server=info,sqlx=warn
EOF
  chmod 600 "$INSTALL_DIR/.env"
  echo "IMPORTANT: Edit $INSTALL_DIR/.env and set POSTGRES_PASSWORD + DATABASE_URL before starting!"
fi

chown -R "$SERVICE_USER:$SERVICE_USER" "$INSTALL_DIR"
chmod 600 "$INSTALL_DIR/.env"

echo "==> Installing systemd unit..."
install -m 0644 packaging/server/twg-server.service /etc/systemd/system/twg-server.service
systemctl daemon-reload
systemctl enable twg-server

if grep -q "CHANGE_ME_STRONG_DB_PASSWORD" "$INSTALL_DIR/.env"; then
  echo "==> Skipping start because .env still contains placeholder password."
  echo "    Edit $INSTALL_DIR/.env, start PostgreSQL, then run: sudo systemctl start twg-server"
else
  systemctl restart twg-server
  sleep 2
  systemctl status twg-server --no-pager
fi
