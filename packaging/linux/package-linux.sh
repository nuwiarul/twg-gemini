#!/usr/bin/env bash
set -euo pipefail

# Build & package twg-app for Linux (.tar.gz + optional .deb & .AppImage)
# Direct native binary — no .sh launcher wrapper required.
# Usage:
#   bash packaging/linux/package-linux.sh \
#     [--brand polda-bali] [--public-server-url https://twg.domain-anda.com] \
#     [--brand-dir packaging/brands/polda-bali] [--logos-dir ...] \
#     [--org-name "POLDA BALI"] [--org-subtitle "Biro Ops • Bid TIK"] \
#     [--app-title "Tactical Wall Game 3D"] [--version 0.1.0] [--skip-build]

BRAND=""
PUBLIC_SERVER_URL=""
BRAND_DIR=""
LOGOS_DIR=""
ORG_NAME=""
ORG_SUBTITLE=""
APP_TITLE=""
VERSION="0.1.0"
SKIP_BUILD=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --brand) BRAND="$2"; shift 2 ;;
    --public-server-url|--server-url) PUBLIC_SERVER_URL="$2"; shift 2 ;;
    --brand-dir) BRAND_DIR="$2"; shift 2 ;;
    --logos-dir) LOGOS_DIR="$2"; shift 2 ;;
    --org-name) ORG_NAME="$2"; shift 2 ;;
    --org-subtitle) ORG_SUBTITLE="$2"; shift 2 ;;
    --app-title) APP_TITLE="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

if [[ -n "$BRAND" ]]; then export TWG_BRAND="$BRAND"; fi
if [[ -n "$BRAND_DIR" ]]; then export TWG_BRAND_DIR="$BRAND_DIR"; fi
if [[ -n "$LOGOS_DIR" ]]; then export TWG_BRAND_LOGOS_DIR="$LOGOS_DIR"; fi
if [[ -n "$PUBLIC_SERVER_URL" ]]; then export TWG_PUBLIC_SERVER_URL="$PUBLIC_SERVER_URL"; fi
if [[ -n "$ORG_NAME" ]]; then export TWG_ORG_NAME="$ORG_NAME"; fi
if [[ -n "$ORG_SUBTITLE" ]]; then export TWG_ORG_SUBTITLE="$ORG_SUBTITLE"; fi
if [[ -n "$APP_TITLE" ]]; then export TWG_APP_TITLE="$APP_TITLE"; fi

if [[ -n "${TWG_BRAND_DIR:-}" ]]; then
  RESOLVED_BRAND_DIR="$TWG_BRAND_DIR"
elif [[ -n "${TWG_BRAND:-}" ]]; then
  RESOLVED_BRAND_DIR="$REPO_ROOT/packaging/brands/$TWG_BRAND"
else
  RESOLVED_BRAND_DIR="$REPO_ROOT/packaging/brands/polda-bali"
fi
if [[ ! -d "$RESOLVED_BRAND_DIR" ]]; then
  echo "Brand profile folder not found: $RESOLVED_BRAND_DIR" >&2
  exit 1
fi
BRAND_SLUG="$(basename "$RESOLVED_BRAND_DIR")"

if [[ $SKIP_BUILD -eq 0 ]]; then
  echo "==> Building twg-app (release) for Linux [brand: $BRAND_SLUG]..."
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

# Install native binary directly (no wrapper script)
install -m 0755 "$BIN_PATH" "$STAGE_DIR/twg-app"
cp "$SCRIPT_DIR/twg-app.desktop" "$STAGE_DIR/twg-app.desktop"
ICONS_DIR="$REPO_ROOT/packaging/icons"
if [[ -f "$ICONS_DIR/twg-app-256.png" ]]; then
  cp "$ICONS_DIR/twg-app-256.png" "$STAGE_DIR/twg-app.png"
fi

# Bundle sidecar ffmpeg & ffprobe into portable tarball when available (FFMPEG_BIN_DIR or PATH)
FFMPEG_SRC="${FFMPEG_BIN_DIR:+${FFMPEG_BIN_DIR}/ffmpeg}"
FFPROBE_SRC="${FFMPEG_BIN_DIR:+${FFMPEG_BIN_DIR}/ffprobe}"
if [[ -z "$FFMPEG_SRC" ]] && command -v ffmpeg >/dev/null 2>&1; then
  FFMPEG_SRC="$(command -v ffmpeg)"
fi
if [[ -z "$FFPROBE_SRC" ]] && command -v ffprobe >/dev/null 2>&1; then
  FFPROBE_SRC="$(command -v ffprobe)"
fi
if [[ -n "$FFMPEG_SRC" && -f "$FFMPEG_SRC" && -n "$FFPROBE_SRC" && -f "$FFPROBE_SRC" ]]; then
  echo "==> Bundling FFmpeg sidecar into $STAGE_DIR/bin..."
  mkdir -p "$STAGE_DIR/bin"
  install -m 0755 "$FFMPEG_SRC" "$STAGE_DIR/bin/ffmpeg"
  install -m 0755 "$FFPROBE_SRC" "$STAGE_DIR/bin/ffprobe"
fi

TAR_PATH="$DIST_DIR/twg-app-${BRAND_SLUG}-linux-${ARCH}-v${VERSION}.tar.gz"
echo "==> Creating portable tarball: $TAR_PATH"
tar -C "$DIST_DIR" -czf "$TAR_PATH" "twg-app-linux-${ARCH}"

# Optional .deb via cargo-deb
if command -v cargo-deb >/dev/null 2>&1; then
  echo "==> Building Debian package (.deb) via cargo-deb..."
  cargo deb -p twg-app --no-build --no-strip -o "$DIST_DIR/twg-app-${BRAND_SLUG}_${VERSION}_${ARCH}.deb"
else
  echo "==> cargo-deb not installed — skipped .deb (install with: cargo install cargo-deb)"
fi

# Optional .AppImage via appimagetool (direct native binary AppRun symlink)
if command -v appimagetool >/dev/null 2>&1; then
  echo "==> Building AppImage..."
  APPDIR="$DIST_DIR/TWG-3D.AppDir"
  rm -rf "$APPDIR"
  mkdir -p "$APPDIR/usr/bin"
  install -m 0755 "$BIN_PATH" "$APPDIR/usr/bin/twg-app"
  ln -sf "usr/bin/twg-app" "$APPDIR/AppRun"
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
  ARCH="$ARCH" appimagetool "$APPDIR" "$DIST_DIR/TWG-3D-${BRAND_SLUG}-${ARCH}-v${VERSION}.AppImage"
fi

echo "==> Done! Linux artifacts in $DIST_DIR"
