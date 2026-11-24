#!/usr/bin/env bash
set -euo pipefail

# Build & package twg-app for macOS (.app bundle + .dmg)
# Usage (run on macOS):
#   bash packaging/macos/package-macos.sh [--arch arm64|x64|universal] [--server-url https://twg.domain-anda.com] [--version 0.1.0] [--sign-identity "-"] [--skip-build]

ARCH="arm64"
SERVER_URL="https://twg.domain-anda.com"
VERSION="0.1.0"
SIGN_IDENTITY="-"
SKIP_BUILD=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --server-url) SERVER_URL="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --sign-identity) SIGN_IDENTITY="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

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

install -m 0755 "$FINAL_BIN" "$APP_BUNDLE/Contents/MacOS/twg-app-bin"
sed "s|__SERVER_URL__|${SERVER_URL}|g" "$SCRIPT_DIR/twg-app-launcher.sh" > "$APP_BUNDLE/Contents/MacOS/twg-app"
chmod +x "$APP_BUNDLE/Contents/MacOS/twg-app"

sed "s|__VERSION__|${VERSION}|g" "$SCRIPT_DIR/Info.plist" > "$APP_BUNDLE/Contents/Info.plist"

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

DMG_PATH="$REPO_ROOT/dist/TWG-3D-macOS-${ARCH}-v${VERSION}.dmg"
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

echo "==> Done! macOS artifacts:"
echo "    App Bundle : $APP_BUNDLE"
echo "    DMG Image  : $DMG_PATH"
