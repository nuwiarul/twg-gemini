#!/usr/bin/env bash
set -euo pipefail

# Build & package twg-app for Linux (.tar.gz + optional .deb & .AppImage)
# Usage:
#   bash packaging/linux/package-linux.sh [--server-url https://twg.domain-anda.com] [--version 0.1.0] [--skip-build]

SERVER_URL="https://twg.domain-anda.com"
VERSION="0.1.0"
SKIP_BUILD=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --server-url) SERVER_URL="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

if [[ $SKIP_BUILD -eq 0 ]]; then
  echo "==> Building twg-app (release) for Linux..."
  cargo build --release -p twg-app
fi

BIN_PATH="$REPO_ROOT/target/release/twg-app"
if [[ ! -f "$BIN_PATH" ]]; then
  echo "Binary not found at $BIN_PATH" >&2
  exit 1
fi

ARCH="$(uname -m)"
DIST_DIR="$REPO_ROOT/dist"
STAGE_DIR="$DIST_DIR/twg-app-linux-${ARCH}"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"

install -m 0755 "$BIN_PATH" "$STAGE_DIR/twg-app-bin"
sed "s|https://twg.domain-anda.com|${SERVER_URL}|g" "$SCRIPT_DIR/twg-app" > "$STAGE_DIR/twg-app.sh"
chmod +x "$STAGE_DIR/twg-app.sh"
cp "$SCRIPT_DIR/twg-app.desktop" "$STAGE_DIR/twg-app.desktop"
ICONS_DIR="$REPO_ROOT/packaging/icons"
if [[ -f "$ICONS_DIR/twg-app-256.png" ]]; then
  cp "$ICONS_DIR/twg-app-256.png" "$STAGE_DIR/twg-app.png"
fi

TAR_PATH="$DIST_DIR/twg-app-linux-${ARCH}-v${VERSION}.tar.gz"
echo "==> Creating portable tarball: $TAR_PATH"
tar -C "$DIST_DIR" -czf "$TAR_PATH" "twg-app-linux-${ARCH}"

# Optional .deb via cargo-deb
if command -v cargo-deb >/dev/null 2>&1; then
  echo "==> Building Debian package (.deb) via cargo-deb..."
  cargo deb -p twg-app --no-build --no-strip -o "$DIST_DIR/twg-app_${VERSION}_${ARCH}.deb"
else
  echo "==> cargo-deb not installed — skipped .deb (install with: cargo install cargo-deb)"
fi

# Optional .AppImage via appimagetool
if command -v appimagetool >/dev/null 2>&1; then
  echo "==> Building AppImage..."
  APPDIR="$DIST_DIR/TWG-3D.AppDir"
  rm -rf "$APPDIR"
  mkdir -p "$APPDIR/usr/bin"
  install -m 0755 "$BIN_PATH" "$APPDIR/usr/bin/twg-app-bin"
  cp "$SCRIPT_DIR/twg-app.desktop" "$APPDIR/twg-app.desktop"
  if [[ -f "$ICONS_DIR/twg-app-256.png" ]]; then
    cp "$ICONS_DIR/twg-app-256.png" "$APPDIR/twg-app.png"
    cp "$ICONS_DIR/twg-app-256.png" "$APPDIR/.DirIcon"
  fi
  for sz in 48 64 128 256 512; do
    if [[ -f "$ICONS_DIR/twg-app-${sz}.png" ]]; then
      mkdir -p "$APPDIR/usr/share/icons/hicolor/${sz}x${sz}/apps"
      cp "$ICONS_DIR/twg-app-${sz}.png" "$APPDIR/usr/share/icons/hicolor/${sz}x${sz}/apps/twg-app.png"
    fi
  done
  cat <<EOF > "$APPDIR/AppRun"
#!/usr/bin/env bash
set -euo pipefail
HERE="\$(dirname "\$(readlink -f "\${0}")")"
DATA_DIR="\${XDG_DATA_HOME:-\$HOME/.local/share}/twg-app"
export TWG_API_URL="\${TWG_API_URL:-${SERVER_URL}}"
export TWG_LOCAL_ASSETS_DIR="\${TWG_LOCAL_ASSETS_DIR:-\$DATA_DIR/offline_assets}"
export TWG_TILE_CACHE_DIR="\${TWG_TILE_CACHE_DIR:-\$DATA_DIR/tile_cache}"
mkdir -p "\$DATA_DIR" "\$TWG_LOCAL_ASSETS_DIR" "\$TWG_TILE_CACHE_DIR"
cd "\$DATA_DIR"
exec "\$HERE/usr/bin/twg-app-bin" "\$@"
EOF
  chmod +x "$APPDIR/AppRun"
  ARCH="$ARCH" appimagetool "$APPDIR" "$DIST_DIR/TWG-3D-${ARCH}-v${VERSION}.AppImage"
fi

echo "==> Done! Linux artifacts in $DIST_DIR"
