#!/usr/bin/env bash
set -euo pipefail

# Build & package twg-app for macOS (.app bundle + .dmg)
# Direct native binary inside Contents/MacOS/twg-app — no shell launcher wrapper required.
# Usage (run on macOS):
#   bash packaging/macos/package-macos.sh \
#     [--brand polda-bali] [--arch arm64|x64|universal] \
#     [--public-server-url https://twg.domain-anda.com] \
#     [--brand-dir packaging/brands/polda-bali] [--logos-dir ...] \
#     [--org-name "POLDA BALI"] [--org-subtitle "Biro Ops • Bid TIK"] \
#     [--app-title "Tactical Wall Game 3D"] [--version 0.1.0] [--sign-identity "-"] [--skip-build]

BRAND=""
ARCH="arm64"
PUBLIC_SERVER_URL=""
BRAND_DIR=""
LOGOS_DIR=""
ORG_NAME=""
ORG_SUBTITLE=""
APP_TITLE=""
VERSION="0.1.0"
SIGN_IDENTITY="-"
SKIP_BUILD=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --brand) BRAND="$2"; shift 2 ;;
    --arch) ARCH="$2"; shift 2 ;;
    --public-server-url|--server-url) PUBLIC_SERVER_URL="$2"; shift 2 ;;
    --brand-dir) BRAND_DIR="$2"; shift 2 ;;
    --logos-dir) LOGOS_DIR="$2"; shift 2 ;;
    --org-name) ORG_NAME="$2"; shift 2 ;;
    --org-subtitle) ORG_SUBTITLE="$2"; shift 2 ;;
    --app-title) APP_TITLE="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --sign-identity) SIGN_IDENTITY="$2"; shift 2 ;;
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

DIST_DIR="$REPO_ROOT/dist/macos"
mkdir -p "$DIST_DIR"

FINAL_BIN=""
case "$ARCH" in
  arm64)
    if [[ $SKIP_BUILD -eq 0 ]]; then
      rustup target add aarch64-apple-darwin
      cargo build --release -p twg-app --target aarch64-apple-darwin
    fi
    FINAL_BIN="$REPO_ROOT/target/aarch64-apple-darwin/release/twg-app"
    ;;
  x64)
    if [[ $SKIP_BUILD -eq 0 ]]; then
      rustup target add x86_64-apple-darwin
      cargo build --release -p twg-app --target x86_64-apple-darwin
    fi
    FINAL_BIN="$REPO_ROOT/target/x86_64-apple-darwin/release/twg-app"
    ;;
  universal)
    if [[ $SKIP_BUILD -eq 0 ]]; then
      rustup target add aarch64-apple-darwin x86_64-apple-darwin
      cargo build --release -p twg-app --target aarch64-apple-darwin
      cargo build --release -p twg-app --target x86_64-apple-darwin
    fi
    FINAL_BIN="$DIST_DIR/twg-app-universal"
    lipo -create \
      "$REPO_ROOT/target/aarch64-apple-darwin/release/twg-app" \
      "$REPO_ROOT/target/x86_64-apple-darwin/release/twg-app" \
      -output "$FINAL_BIN"
    ;;
  *)
    echo "Unsupported --arch '$ARCH'. Use arm64, x64, or universal." >&2
    exit 1
    ;;
esac

APP_BUNDLE="$DIST_DIR/TWG-3D.app"
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS" "$APP_BUNDLE/Contents/Resources"

# Install native binary directly as CFBundleExecutable (no shell launcher script)
install -m 0755 "$FINAL_BIN" "$APP_BUNDLE/Contents/MacOS/twg-app"

# Bundle sidecar ffmpeg & ffprobe inside Contents/Resources/bin/ when available (auto-discovered by twg-app)
FFMPEG_SRC="${FFMPEG_BIN_DIR:+${FFMPEG_BIN_DIR}/ffmpeg}"
FFPROBE_SRC="${FFMPEG_BIN_DIR:+${FFMPEG_BIN_DIR}/ffprobe}"
if [[ -z "$FFMPEG_SRC" ]] && command -v ffmpeg >/dev/null 2>&1; then
  FFMPEG_SRC="$(command -v ffmpeg)"
fi
if [[ -z "$FFPROBE_SRC" ]] && command -v ffprobe >/dev/null 2>&1; then
  FFPROBE_SRC="$(command -v ffprobe)"
fi
if [[ -n "$FFMPEG_SRC" && -f "$FFMPEG_SRC" && -n "$FFPROBE_SRC" && -f "$FFPROBE_SRC" ]]; then
  echo "==> Bundling FFmpeg sidecar into $APP_BUNDLE/Contents/Resources/bin..."
  mkdir -p "$APP_BUNDLE/Contents/Resources/bin"
  install -m 0755 "$FFMPEG_SRC" "$APP_BUNDLE/Contents/Resources/bin/ffmpeg"
  install -m 0755 "$FFPROBE_SRC" "$APP_BUNDLE/Contents/Resources/bin/ffprobe"
fi

sed "s|__VERSION__|${VERSION}|g" "$SCRIPT_DIR/Info.plist" > "$APP_BUNDLE/Contents/Info.plist"

# Bundle macOS app icon (.icns) — regenerate natively with iconutil when available, else use pre-built .icns
ICONS_DIR="$REPO_ROOT/packaging/icons"
APP_ICNS_DST="$APP_BUNDLE/Contents/Resources/twg-app.icns"
if command -v iconutil >/dev/null 2>&1 && command -v sips >/dev/null 2>&1 && [[ -f "$ICONS_DIR/twg-app-macos.png" ]]; then
  echo "==> Generating native macOS .icns via iconutil..."
  ICONSET_DIR="$DIST_DIR/twg-app.iconset"
  rm -rf "$ICONSET_DIR"
  mkdir -p "$ICONSET_DIR"
  sips -z 16 16     "$ICONS_DIR/twg-app-macos.png" --out "$ICONSET_DIR/icon_16x16.png" >/dev/null
  sips -z 32 32     "$ICONS_DIR/twg-app-macos.png" --out "$ICONSET_DIR/icon_16x16@2x.png" >/dev/null
  sips -z 32 32     "$ICONS_DIR/twg-app-macos.png" --out "$ICONSET_DIR/icon_32x32.png" >/dev/null
  sips -z 64 64     "$ICONS_DIR/twg-app-macos.png" --out "$ICONSET_DIR/icon_32x32@2x.png" >/dev/null
  sips -z 128 128   "$ICONS_DIR/twg-app-macos.png" --out "$ICONSET_DIR/icon_128x128.png" >/dev/null
  sips -z 256 256   "$ICONS_DIR/twg-app-macos.png" --out "$ICONSET_DIR/icon_128x128@2x.png" >/dev/null
  sips -z 256 256   "$ICONS_DIR/twg-app-macos.png" --out "$ICONSET_DIR/icon_256x256.png" >/dev/null
  sips -z 512 512   "$ICONS_DIR/twg-app-macos.png" --out "$ICONSET_DIR/icon_256x256@2x.png" >/dev/null
  sips -z 512 512   "$ICONS_DIR/twg-app-macos.png" --out "$ICONSET_DIR/icon_512x512.png" >/dev/null
  cp "$ICONS_DIR/twg-app-macos.png" "$ICONSET_DIR/icon_512x512@2x.png"
  iconutil -c icns "$ICONSET_DIR" -o "$APP_ICNS_DST"
  rm -rf "$ICONSET_DIR"
elif [[ -f "$ICONS_DIR/twg-app.icns" ]]; then
  cp "$ICONS_DIR/twg-app.icns" "$APP_ICNS_DST"
fi

echo "==> Code signing $APP_BUNDLE (identity: $SIGN_IDENTITY)..."
if [[ "$SIGN_IDENTITY" == "-" ]]; then
  codesign --deep --force --sign - "$APP_BUNDLE"
else
  codesign --deep --force --options runtime --sign "$SIGN_IDENTITY" "$APP_BUNDLE"
fi

DMG_STAGE="$DIST_DIR/dmg_stage"
rm -rf "$DMG_STAGE"
mkdir -p "$DMG_STAGE"
cp -R "$APP_BUNDLE" "$DMG_STAGE/"
ln -sf /Applications "$DMG_STAGE/Applications"
if [[ -f "$APP_ICNS_DST" ]]; then
  cp "$APP_ICNS_DST" "$DMG_STAGE/.VolumeIcon.icns"
  if command -v SetFile >/dev/null 2>&1; then
    SetFile -a C "$DMG_STAGE" || true
  fi
fi

DMG_PATH="$REPO_ROOT/dist/TWG-3D-${BRAND_SLUG}-macOS-${ARCH}-v${VERSION}.dmg"
rm -f "$DMG_PATH"
echo "==> Creating DMG installer: $DMG_PATH"
hdiutil create \
  -volname "Tactical Wall Game 3D" \
  -srcfolder "$DMG_STAGE" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

if [[ "$SIGN_IDENTITY" != "-" ]]; then
  codesign --force --sign "$SIGN_IDENTITY" "$DMG_PATH"
fi

echo "==> Done! macOS artifacts [brand: $BRAND_SLUG]:"
echo "    App Bundle : $APP_BUNDLE"
echo "    DMG Image  : $DMG_PATH"
