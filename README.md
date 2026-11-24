# Tactical Wall Game 3D — Full Rust (Desktop-First, WASM-Ready)

A 100% Rust rewrite of the Tactical Wall Game platform featuring:
- **PostgreSQL 18 on Podman Desktop** (`compose.yaml`, clean schema & SQLx migrations)
- **Axum Backend (`crates/twg-server`)**: PASETO v4 local authentication, Argon2id password hashing, 5-role satker-scoped RBAC, scenario bundle API, multipart uploads, and a **Native Axum WebSocket Hub** (no external Centrifugo required).
- **Shared Core (`crates/twg-shared`)**: `wasm32-unknown-unknown`-compatible DTOs, WebSocket protocol, and `f64` Web Mercator (`EPSG:3857`) projection & slippy-tile math.
- **Bevy 3D Desktop Frontend (`crates/twg-app`)**:
  - Live async slippy-tile streaming for **OpenStreetMap (OSM)** and **ArcGIS World Imagery Satellite** with local disk cache (`.tile_cache/`).
  - **Indoor Tactical Grid** mode (`GridDark`, `GridLight`, `BlankDark`, `BlankLight`).
  - Native **glTF / `.glb`** 3D unit/vehicle/building rendering (`WorldAssetRoot` + procedural 3D fallback for unmigrated catalog models).
  - 3D extruded building polygons (`earcutr`), tactical plot lines, distance measurement tool, projected 3D text labels, route animation playback timeline, and real-time WebSocket operator sync.

## Architecture

```text
f:\twg-gemini
├── compose.yaml               # Podman Desktop PostgreSQL 18.4 (port 5435)
├── assets/
│   ├── catalog/models.json    # Migrated 3D model catalog (311 entries)
│   ├── models/                # Standard non-Draco .glb models
│   └── uploads/               # Runtime uploaded files
├── scripts/
│   ├── dev-up.ps1             # Start Podman PostgreSQL & wait until healthy
│   └── migrate-assets.mjs     # Asset migration (Draco-free filtering + optional decompression)
└── crates/
    ├── twg-shared/            # Pure Rust DTOs, WS protocol, Web Mercator geo math (WASM-ready)
    ├── twg-server/            # Axum 0.8 + SQLx 0.9 + PASETO v4 + Argon2id + WS Hub
    └── twg-app/               # Bevy 0.20 + bevy_egui 0.43 Desktop 3D Tactical Client
```

## Quick Start

### 1. Start PostgreSQL on Podman Desktop
```powershell
powershell -ExecutionPolicy Bypass -File scripts/dev-up.ps1
```
*(Or `podman compose up -d` — listens on `127.0.0.1:5435`, DB `twg_rust`, user `twg`.)*

### 2. Start the Rust Backend
```powershell
cargo run -p twg-server
```
On first startup against an empty DB, `twg-server` runs SQLx migrations and seeds:
- **Admin:** `admin` / `admin123`
- **Operator (`user_satker`):** `operator` / `operator123`
- **Demo Scenario:** *Pengamanan Sektor Denpasar (Demo 3D)* with 3D glTF units, an extruded 3D command building, perimeter plot line, text labels, and a patrol route animation.

### 3. Start the Bevy 3D Desktop Application
In a second terminal:
```powershell
cargo run -p twg-app
```
Or click **Offline 3D Map Preview (OSM / ArcGIS + glTF)** on the login screen to explore the 3D OSM/ArcGIS map & glTF assets without the server.

## Migrating More 3D `.glb` Assets
By default, `node scripts/migrate-assets.mjs` migrates a curated starter pack of 35 `.glb` models from `E:\tactical_game\3dmap\public`, **automatically decoding Draco (`KHR_draco_mesh_compression`)** via `@gltf-transform/cli` and **patching mismatched embedded texture `mimeType` headers** (`image/jpeg` -> `image/png`) so Bevy loads every model cleanly.
- Migrate all 311 catalog models (auto-decoding Draco + fixing image MIME types):
  ```powershell
  node scripts/migrate-assets.mjs --all
  ```
- Skip Draco decoding and only migrate uncompressed models:
  ```powershell
  node scripts/migrate-assets.mjs --all --skip-draco
  ```

## Controls (3D Map)
- **Left-drag (Select mode)** or **Middle-drag**: Pan map
- **Right-drag**: Orbit bearing & pitch
- **Mouse Wheel**: Zoom in / out
- **WASD / Arrows**: Pan • **Q / E**: Rotate bearing • **R**: Reset camera
- **Shift + Left-drag** (or **Move** tool): Drag selected 3D marker and sync over WebSocket
- **Enter / Esc**: Finish or cancel Draw Line / Draw Building

## Phase 2 (WASM Web Frontend Path)
`crates/twg-shared` already compiles cleanly for `wasm32-unknown-unknown`. In Phase 2, swap `twg-app`'s `reqwest` + `tokio-tungstenite` worker adapters behind `#[cfg(target_arch = "wasm32")]` for `gloo-net` / `ehttp` + Web `WebSocket` and build with `trunk` (`bevy` + `bevy_egui` support WebGPU/WebGL2 out of the box).
