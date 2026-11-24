#!/usr/bin/env bash
set -euo pipefail

MACOS_DIR="$(cd "$(dirname "$0")" && pwd)"
DATA_DIR="$HOME/Library/Application Support/TWG-3D"
export TWG_API_URL="${TWG_API_URL:-__SERVER_URL__}"
export TWG_LOCAL_ASSETS_DIR="${TWG_LOCAL_ASSETS_DIR:-$DATA_DIR/offline_assets}"
export TWG_TILE_CACHE_DIR="${TWG_TILE_CACHE_DIR:-$DATA_DIR/tile_cache}"

mkdir -p "$DATA_DIR" "$TWG_LOCAL_ASSETS_DIR" "$TWG_TILE_CACHE_DIR"
cd "$DATA_DIR"

exec "$MACOS_DIR/twg-app-bin" "$@"
