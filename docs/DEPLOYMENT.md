# Panduan Deploy `twg-server` (VPS) & Packaging `twg-app` (Windows, Linux, macOS)

Dokumentasi lengkap untuk menjalankan **Tactical Wall Game 3D (`twg-server`)** secara production di VPS Linux dan memaketkan aplikasi desktop **`twg-app`** (Bevy 0.20 + `bevy_egui`) untuk **Windows**, **Linux**, dan **macOS**.

## File Packaging & Deploy Siap Pakai di Repository

| Target | File | Cara Pakai Cepat |
| --- | --- | --- |
| **VPS Server (Deploy)** | `packaging/server/deploy-vps.sh` | `sudo bash packaging/server/deploy-vps.sh --with-assets` |
| **VPS Server (Systemd)** | `packaging/server/twg-server.service` | Disalin otomatis ke `/etc/systemd/system/twg-server.service` |
| **VPS Server (Reverse Proxy)** | `packaging/server/nginx-twg-server.conf` / `Caddyfile` | Copy ke `/etc/nginx/sites-available/` atau `/etc/caddy/Caddyfile` |
| **VPS Server (PostgreSQL 18)** | `packaging/server/compose.prod.yaml` | `docker compose -f compose.prod.yaml up -d` |
| **Windows (`.zip` + `.exe`)** | `packaging/windows/package-windows.ps1` & `twg-app.iss` | `powershell -ExecutionPolicy Bypass -File packaging/windows/package-windows.ps1 -ServerUrl https://twg.domain-anda.com` |
| **Linux (`.tar.gz` / `.deb` / `.AppImage`)** | `packaging/linux/package-linux.sh`, `twg-app`, `twg-app.desktop` | `bash packaging/linux/package-linux.sh --server-url https://twg.domain-anda.com` |
| **macOS (`.app` + `.dmg`)** | `packaging/macos/package-macos.sh`, `Info.plist`, `twg-app-launcher.sh` | `bash packaging/macos/package-macos.sh --arch universal --server-url https://twg.domain-anda.com` |
| **CI/CD Multi-OS Otomatis** | `.github/workflows/release-app.yml` | Push tag `git tag v0.1.0 && git push origin v0.1.0` atau jalankan manual dari tab Actions |

---

## Ringkasan Arsitektur Production

```text
┌─────────────────────────────────────────────────────────────┐
│ Desktop Clients (twg-app — Windows / Linux / macOS)         │
│ • HTTPS REST API + Offline 3D Model Sync (SHA-256 manifest) │
│ • WSS Live Ops WebSocket (/ws)                              │
│ • Cache lokal: offline_assets/ (864 GLB) & .tile_cache/     │
└──────────────────────────┬──────────────────────────────────┘
                           │ HTTPS (443) / WSS (443)
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ VPS Linux (Ubuntu 22.04 / 24.04 LTS)                        │
│                                                             │
│  [Nginx / Caddy Reverse Proxy + SSL Let's Encrypt]          │
│         │  (client_max_body_size 130M, WS Upgrade)          │
│         ▼                                                   │
│  [twg-server (systemd) — 127.0.0.1:8010]                    │
│    ├── PostgreSQL 18 (127.0.0.1:5435)                       │
│    ├── /opt/twg-server/assets/ (models/ 311 model + LOD2)   │
│    └── Node.js + @gltf-transform/cli                        │
│        (konversi otomatis LOD0+LOD2 saat Admin upload)      │
└─────────────────────────────────────────────────────────────┘
```

> **Catatan TLS/HTTPS:** `twg-app` sudah menggunakan `rustls` untuk HTTP (`reqwest`) dan WebSocket (`tokio-tungstenite`), serta otomatis mengubah `https://` menjadi `wss://`. Sertifikat SSL standar seperti **Let's Encrypt** langsung bekerja di Windows, Linux, dan macOS tanpa perlu instalasi OpenSSL tambahan di PC pengguna.

---

# Bagian 1 — Deploy `twg-server` di VPS Linux

## 1.1 Spesifikasi VPS Minimum

| Komponen | Minimum | Rekomendasi Production |
| --- | --- | --- |
| OS | Ubuntu 22.04 / 24.04 LTS (64-bit) | Ubuntu 24.04 LTS |
| CPU | 2 vCPU | 4 vCPU (konversi glTF + kompresi Sharp lebih cepat) |
| RAM | 2 GB | 4 GB |
| Storage | 20 GB SSD | 40 GB+ SSD (311 model LOD0+LOD2 = ~435 MB + ruang pertumbuhan) |
| Port Publik | `22` (SSH), `80` (HTTP), `443` (HTTPS) | Jangan buka port `8010` atau `5432`/`5435` ke internet |

---

## 1.2 Persiapan VPS & Dependencies

Login SSH ke VPS, lalu perbarui sistem dan pasang paket dasar:

```bash
sudo apt update && sudo apt upgrade -y
sudo apt install -y curl git build-essential pkg-config libssl-dev ufw rsync ca-certificates
```

### Buat User Sistem Khusus (`twg`)

```bash
sudo useradd --system --create-home --shell /bin/bash twg
sudo mkdir -p /opt/twg-server/assets/uploads
sudo chown -R twg:twg /opt/twg-server
```

### Konfigurasi Firewall (`ufw`)

```bash
sudo ufw allow OpenSSH
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw enable
sudo ufw status
```

---

## 1.3 Instalasi PostgreSQL 18

Pilih **salah satu** metode berikut:

### Opsi A (Direkomendasikan): Docker / Podman Compose (sesuai `compose.yaml` repo)

```bash
# Install Docker Engine + Compose plugin (Ubuntu)
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker $USER
```

Buat `/opt/twg-server/compose.yaml` (pastikan binding ke `127.0.0.1` agar tidak terbuka ke publik):

```yaml
services:
  postgres:
    image: docker.io/library/postgres:18.4-alpine
    container_name: twg-rust-postgres
    restart: unless-stopped
    environment:
      POSTGRES_DB: twg_rust
      POSTGRES_USER: twg
      POSTGRES_PASSWORD: GANTI_PASSWORD_DB_YANG_KUAT
    ports:
      - "127.0.0.1:5435:5432"
    volumes:
      - twg_rust_pgdata:/var/lib/postgresql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U twg -d twg_rust"]
      interval: 5s
      timeout: 5s
      retries: 10

volumes:
  twg_rust_pgdata:
```

Jalankan:

```bash
cd /opt/twg-server
docker compose up -d
docker compose ps
```

### Opsi B: PostgreSQL Native di Ubuntu

Jika memakai PostgreSQL native (port `5432`), buat user dan database:

```bash
sudo -u postgres psql -c "CREATE USER twg WITH PASSWORD 'GANTI_PASSWORD_DB_YANG_KUAT';"
sudo -u postgres psql -c "CREATE DATABASE twg_rust OWNER twg;"
```

Lalu sesuaikan port pada `DATABASE_URL` menjadi `5432`.

---

## 1.4 Instalasi Node.js & `@gltf-transform/cli` (Wajib untuk Auto-Convert Upload Admin)

Saat Admin menambah/mengedit model 3D dari `twg-app`, `twg-server` memanggil `node scripts/convert-single-model.mjs` untuk:
- Mendekode kompresi Draco (`KHR_draco_mesh_compression`) ke format standar Bevy,
- Memvalidasi animasi `*_idle` (0 animasi) dan `*_walk` (>= 1 animasi),
- Mengonversi `.gltf` menjadi `.glb`,
- Memperbaiki header MIME tekstur, dan
- Menghasilkan file **LOD0** + **LOD2** secara otomatis.

Pasang Node.js 22/24 LTS dan `@gltf-transform/cli` secara global di VPS:

```bash
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt install -y nodejs
sudo npm install -g @gltf-transform/cli

# Verifikasi instalasi
node -v
gltf-transform --version
npm root -g
```

---

## 1.5 Build & Upload `twg-server` + Aset 311 Model

### Opsi A: Build Langsung di VPS

```bash
# Pasang Rust toolchain di VPS (jika belum)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Clone / upload repo ke direktori build sementara
git clone <URL_REPO_ANDA> /tmp/twg-build
cd /tmp/twg-build

# Build binary release teroptimasi
cargo build --release -p twg-server

# Salin binary & script konversi ke /opt/twg-server
sudo cp target/release/twg-server /opt/twg-server/twg-server
sudo cp -r scripts /opt/twg-server/scripts
```

### Opsi B: Build di Komputer Lokal / CI lalu Kirim ke VPS

Jika komputer lokal memakai Linux x86_64 (atau WSL2 Ubuntu):

```bash
cargo build --release -p twg-server
rsync -avz target/release/twg-server user@IP_VPS:/opt/twg-server/twg-server
rsync -avz scripts/gltf-pipeline.mjs scripts/convert-single-model.mjs user@IP_VPS:/opt/twg-server/scripts/
```

### Upload 311 Model 3D (`assets/models` & `assets/catalog/models.json`)

Dari mesin lokal yang sudah menjalankan `node scripts/convert-models.mjs` (folder `assets/` berisi ~435 MB / 864 file `.glb`):

```bash
# Dari Windows (PowerShell dengan OpenSSH / WSL) atau Linux/macOS:
rsync -avz --progress assets/catalog assets/models user@IP_VPS:/opt/twg-server/assets/
```

*(Jika `rsync` tidak tersedia di Windows, bisa kompres `assets/catalog` dan `assets/models` menjadi `twg-assets.tar.gz`, upload dengan `scp`, lalu ekstrak di `/opt/twg-server/assets/`.)*

Pastikan kepemilikan dan struktur direktori di VPS rapi:

```text
/opt/twg-server/
├── twg-server
├── .env
├── scripts/
│   ├── gltf-pipeline.mjs
│   └── convert-single-model.mjs
└── assets/
    ├── catalog/
    │   └── models.json
    ├── models/
    │   ├── bangunans/
    │   ├── kendaraan_umums/
    │   ├── kendaraans/
    │   ├── peoples/
    │   ├── stackholders/
    │   ├── tools/
    │   └── units/
    └── uploads/
```

```bash
sudo chown -R twg:twg /opt/twg-server
sudo chmod +x /opt/twg-server/twg-server
```

---

## 1.6 Konfigurasi `.env` Production

Buat kunci simetris PASETO v4 acak (64 karakter hex = 32 byte):

```bash
openssl rand -hex 32
```

Buat file `/opt/twg-server/.env`:

```dotenv
DATABASE_URL=postgres://twg:GANTI_PASSWORD_DB_YANG_KUAT@127.0.0.1:5435/twg_rust
SERVER_BIND_ADDR=127.0.0.1:8010
PASETO_SYMMETRIC_KEY=TEMPEL_HASIL_OPENSSL_RAND_HEX_32_DI_SINI
ACCESS_TOKEN_TTL_SECS=86400
REFRESH_TOKEN_TTL_SECS=604800
UPLOAD_DIR=/opt/twg-server/assets/uploads
ASSETS_DIR=/opt/twg-server/assets
CORS_ALLOWED_ORIGINS=*
RUST_LOG=info,twg_server=info,sqlx=warn
```

Kunci izin file `.env`:

```bash
sudo chown twg:twg /opt/twg-server/.env
sudo chmod 600 /opt/twg-server/.env
```

---

## 1.7 Menjalankan `twg-server` sebagai `systemd` Service

Buat `/etc/systemd/system/twg-server.service`:

```ini
[Unit]
Description=Tactical Wall Game 3D Backend Server (twg-server)
After=network-online.target docker.service postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=twg
Group=twg
# WAJIB /opt/twg-server agar scripts/convert-single-model.mjs otomatis ditemukan
WorkingDirectory=/opt/twg-server
EnvironmentFile=/opt/twg-server/.env
Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
ExecStart=/opt/twg-server/twg-server
Restart=always
RestartSec=3
LimitNOFILE=65536

# Hardening dasar
NoNewPrivileges=true
PrivateTmp=false

[Install]
WantedBy=multi-user.target
```

> **Penting:** `PrivateTmp=false` (atau `PrivateTmp=true` tidak masalah selama `/tmp` dapat ditulis user `twg`), karena setiap konversi upload menggunakan direktori sementara unik `std::env::temp_dir()/twg-model-convert-<uuid>` yang otomatis dibersihkan setelah selesai. Pastikan `node` dan `npm` berada di `/usr/bin` atau `/usr/local/bin` (tercakup di `Environment=PATH=...`).

Aktifkan dan jalankan service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now twg-server
sudo systemctl status twg-server
sudo journalctl -u twg-server -n 50 --no-pager
```

Pada startup pertama terhadap database kosong, log harus menampilkan:
- Migrasi SQLx sukses (`initial_schema` & `model_assets`),
- Seed akun awal (`admin` / `admin123` dan `operator` / `operator123`),
- Auto-import **311 model** ke PostgreSQL `model_assets`,
- `twg-server listening on http://127.0.0.1:8010`.

> **Keamanan Wajib:** Setelah server aktif, segera login sebagai `admin` dan `operator` lalu **ganti password default** melalui `PUT /api/v1/profile/password`!

---

## 1.8 Reverse Proxy HTTPS + WebSocket

### Opsi A: Nginx + Certbot (Let's Encrypt)

```bash
sudo apt install -y nginx certbot python3-certbot-nginx
```

Buat `/etc/nginx/sites-available/twg-server`:

```nginx
server {
    listen 80;
    server_name twg.domain-anda.com;

    # Upload multipart model 3D GLB/glTF hingga 128 MB (sesuai DefaultBodyLimit server)
    client_max_body_size 130M;

    # Timeout panjang untuk proses konversi upload LOD0 + LOD2
    proxy_connect_timeout 60s;
    proxy_send_timeout    300s;
    proxy_read_timeout    300s;

    # WebSocket Live Ops (/ws)
    location /ws {
        proxy_pass http://127.0.0.1:8010/ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }

    # Streaming file model 3D untuk offline sync klien
    location /api/v1/assets/models/file/ {
        proxy_pass http://127.0.0.1:8010;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_buffering off;
        add_header Cache-Control "public, max-age=300";
    }

    # REST API & Static Assets
    location / {
        proxy_pass http://127.0.0.1:8010;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

Aktifkan site dan pasang SSL otomatis:

```bash
sudo ln -s /etc/nginx/sites-available/twg-server /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
sudo certbot --nginx -d twg.domain-anda.com
```

### Opsi B: Caddy (Alternatif Super Ringkas dengan Auto-HTTPS)

File `/etc/caddy/Caddyfile`:

```caddyfile
twg.domain-anda.com {
    request_body {
        max_size 130MB
    }
    reverse_proxy 127.0.0.1:8010 {
        flush_interval -1
    }
}
```

Caddy otomatis mengurus sertifikat Let's Encrypt, HTTP/2, dan WebSocket `/ws`.

---

## 1.9 Verifikasi Deploy, Backup, & Update Server

### Verifikasi Endpoint

```bash
curl -s https://twg.domain-anda.com/healthz
curl -s https://twg.domain-anda.com/readyz
curl -s https://twg.domain-anda.com/api/v1/assets/models/manifest | head -c 400
```

Pastikan `total_models` bernilai `311` dan `total_files` bernilai `864`.

### Backup Harian (Database + Aset 3D)

```bash
# Backup database PostgreSQL
docker exec twg-rust-postgres pg_dump -U twg twg_rust | gzip > /backup/twg_db_$(date +%F).sql.gz

# Backup folder assets (model baru/editan admin + uploads)
rsync -a /opt/twg-server/assets/ /backup/twg_assets/
```

### Update Binary `twg-server` Tanpa Mengganggu Data

```bash
sudo systemctl stop twg-server
sudo cp /tmp/twg-server-new /opt/twg-server/twg-server
sudo chown twg:twg /opt/twg-server/twg-server
sudo chmod +x /opt/twg-server/twg-server
sudo systemctl start twg-server
sudo journalctl -u twg-server -n 30 --no-pager
```

---

# Bagian 2 — Packaging `twg-app` (Windows, Linux, macOS)

## 2.1 Hal Penting Sebelum Memaketkan `twg-app`

1. **Tidak perlu membundel 435 MB model `.glb` di dalam installer** — saat `twg-app` pertama kali dijalankan di komputer manapun, ia otomatis menghubungi server (`TWG_API_URL`) dan menyinkronkan seluruh model secara offline.
   - *Opsional (Lokasi Internet Lambat / Air-Gapped)*: Anda boleh menyalin isi folder `client_assets/` (setelah sync 1×) ke folder `offline_assets/` di dalam paket sebagai **pre-seed**; saat dibuka nanti aplikasi hanya mengecek hash SHA-256 dan langsung siap dalam < 1 detik.
2. **Environment Variables Runtime `twg-app`**:
   - `TWG_API_URL` — URL default server VPS yang terisi otomatis di layar login (mis. `https://twg.domain-anda.com`).
   - `TWG_LOCAL_ASSETS_DIR` — Direktori penyimpanan cache offline model 3D.
     - **Sangat penting untuk installer sistem!** Secara default (bila `TWG_LOCAL_ASSETS_DIR` tidak diset), `twg-app` menyimpan model di `<folder_executable>/offline_assets` dan cache peta di `.tile_cache` pada *working directory*.
     - Jika aplikasi dipasang di folder **read-only** (`C:\Program Files\`, `/usr/bin/`, atau di dalam `.app/Contents/MacOS/` di macOS), **gunakan wrapper script** untuk mengarahkan `TWG_LOCAL_ASSETS_DIR` dan `cd` ke folder data user yang *writable*!
3. **Optimasi Release Build (`Cargo.toml`)** *(opsional, memperkecil ukuran binary & meningkatkan FPS 3D)*:
   Tambahkan ke root `Cargo.toml` sebelum build distribusi:
   ```toml
   [profile.release]
   opt-level = 3
   lto = "thin"
   codegen-units = 1
   strip = "symbols"
   ```
4. **Sembunyikan jendela terminal hitam di Windows** *(opsional untuk rilis GUI)*:
   Tambahkan di baris pertama `crates/twg-app/src/main.rs`:
   ```rust
   #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
   ```

---

## 2.2 Packaging untuk Windows (`.zip` Portable & `.exe` Installer)

### Prasyarat Build di Windows
- Windows 10 / 11 (64-bit)
- Rust MSVC toolchain (`x86_64-pc-windows-msvc`) + Visual Studio Build Tools (Desktop development with C++)

### 1. Build Release

```powershell
cargo build --release -p twg-app
```

Hasil binary: `target\release\twg-app.exe`.

### 2. Paket Portable `.zip` (Siap Copy ke PC Lain)

```powershell
$dist = "dist\twg-app-windows-x64"
New-Item -ItemType Directory -Force -Path $dist | Out-Null
Copy-Item "target\release\twg-app.exe" "$dist\twg-app.exe"

@"
@echo off
setlocal
set TWG_API_URL=https://twg.domain-anda.com
set TWG_LOCAL_ASSETS_DIR=%LOCALAPPDATA%\TWG-3D\offline_assets
if not exist "%LOCALAPPDATA%\TWG-3D" mkdir "%LOCALAPPDATA%\TWG-3D"
cd /d "%LOCALAPPDATA%\TWG-3D"
start "" "%~dp0twg-app.exe"
"@ | Set-Content -Encoding ASCII "$dist\TWG-3D.bat"

Compress-Archive -Path "$dist\*" -DestinationPath "dist\twg-app-windows-x64.zip" -Force
```

### 3. Windows Installer `.exe` (Inno Setup — Direkomendasikan)

Pasang [Inno Setup 6](https://jrsoftware.org/isinfo.php), lalu buat file `packaging\windows\twg-app.iss`:

```iss
[Setup]
AppName=Tactical Wall Game 3D
AppVersion=0.1.0
AppPublisher=TWG Team
DefaultDirName={localappdata}\TWG-3D
DefaultGroupName=Tactical Wall Game 3D
PrivilegesRequired=lowest
OutputDir=..\..\dist
OutputBaseFilename=TWG-3D-Setup-x64
Compression=lzma2/max
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible

[Files]
Source: "..\..\target\release\twg-app.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\twg-app-windows-x64\TWG-3D.bat"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Tactical Wall Game 3D"; Filename: "{app}\TWG-3D.bat"; WorkingDir: "{localappdata}\TWG-3D"
Name: "{autodesktop}\Tactical Wall Game 3D"; Filename: "{app}\TWG-3D.bat"; WorkingDir: "{localappdata}\TWG-3D"

[Run]
Filename: "{app}\TWG-3D.bat"; Description: "Jalankan Tactical Wall Game 3D"; Flags: nowait postinstall skipifsilent
```

Compile installer:

```powershell
& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" packaging\windows\twg-app.iss
```

Hasil: `dist\TWG-3D-Setup-x64.exe` (menginstal ke `%LOCALAPPDATA%\TWG-3D` tanpa perlu hak Administrator dan folder cache langsung writable).

> **Persyaratan PC Klien Windows:** GPU mendukung **Vulkan** atau **DirectX 12** (driver GPU terbaru) dan Visual C++ 2015–2022 Redistributable (x64).

---

## 2.3 Packaging untuk Linux (`.tar.gz`, `.deb`, dan `.AppImage`)

### Prasyarat Build di Ubuntu / Debian

Bevy 0.20 + `rfd` (dialog file picker Admin) membutuhkan paket pengembangan berikut saat compile:

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config \
  libx11-dev libasound2-dev libudev-dev \
  libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libvulkan-dev \
  libgtk-3-dev
```

### 1. Build Release

```bash
cargo build --release -p twg-app
```

Hasil binary: `target/release/twg-app`.

### 2. Paket Portable `.tar.gz`

```bash
DIST="dist/twg-app-linux-x64"
mkdir -p "$DIST"
cp target/release/twg-app "$DIST/twg-app-bin"

cat <<'EOF' > "$DIST/twg-app.sh"
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/twg-app"
export TWG_API_URL="${TWG_API_URL:-https://twg.domain-anda.com}"
export TWG_LOCAL_ASSETS_DIR="${TWG_LOCAL_ASSETS_DIR:-$DATA_DIR/offline_assets}"
mkdir -p "$DATA_DIR" "$TWG_LOCAL_ASSETS_DIR"
cd "$DATA_DIR"
exec "$SCRIPT_DIR/twg-app-bin" "$@"
EOF

chmod +x "$DIST/twg-app-bin" "$DIST/twg-app.sh"
tar -C dist -czf dist/twg-app-linux-x64.tar.gz twg-app-linux-x64
```

### 3. Paket Debian / Ubuntu `.deb` (via `cargo-deb`)

```bash
cargo install cargo-deb
```

Tambahkan metadata berikut di `crates/twg-app/Cargo.toml` dan siapkan file wrapper + desktop entry:

```toml
[package.metadata.deb]
maintainer = "TWG Team <admin@domain-anda.com>"
section = "games"
priority = "optional"
depends = "$auto, libvulkan1, libasound2 | libasound2t64, xdg-desktop-portal"
assets = [
    ["target/release/twg-app", "usr/lib/twg-app/twg-app-bin", "755"],
    ["packaging/linux/twg-app", "usr/bin/twg-app", "755"],
    ["packaging/linux/twg-app.desktop", "usr/share/applications/twg-app.desktop", "644"],
]
```

File `packaging/linux/twg-app` (wrapper):

```bash
#!/usr/bin/env bash
set -euo pipefail
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/twg-app"
export TWG_API_URL="${TWG_API_URL:-https://twg.domain-anda.com}"
export TWG_LOCAL_ASSETS_DIR="${TWG_LOCAL_ASSETS_DIR:-$DATA_DIR/offline_assets}"
mkdir -p "$DATA_DIR" "$TWG_LOCAL_ASSETS_DIR"
cd "$DATA_DIR"
exec /usr/lib/twg-app/twg-app-bin "$@"
```

File `packaging/linux/twg-app.desktop`:

```ini
[Desktop Entry]
Name=Tactical Wall Game 3D
Comment=Tactical 3D Map & glTF Model Manager
Exec=twg-app
Terminal=false
Type=Application
Categories=Graphics;Utility;
```

Build `.deb`:

```bash
cargo deb -p twg-app --no-strip
# Output: target/debian/twg-app_0.1.0-1_amd64.deb
```

### 4. Paket `.AppImage` (Universal Linux)

```bash
APPDIR="dist/TWG-3D.AppDir"
mkdir -p "$APPDIR/usr/bin"
cp target/release/twg-app "$APPDIR/usr/bin/twg-app-bin"
cp packaging/linux/twg-app.desktop "$APPDIR/twg-app.desktop"
# Siapkan ikon 256x256 PNG (contoh: packaging/icon.png -> $APPDIR/twg-app.png)

cat <<'EOF' > "$APPDIR/AppRun"
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "${0}")")"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/twg-app"
export TWG_API_URL="${TWG_API_URL:-https://twg.domain-anda.com}"
export TWG_LOCAL_ASSETS_DIR="${TWG_LOCAL_ASSETS_DIR:-$DATA_DIR/offline_assets}"
mkdir -p "$DATA_DIR" "$TWG_LOCAL_ASSETS_DIR"
cd "$DATA_DIR"
exec "$HERE/usr/bin/twg-app-bin" "$@"
EOF
chmod +x "$APPDIR/AppRun" "$APPDIR/usr/bin/twg-app-bin"

# Download & jalankan appimagetool
curl -L -o /tmp/appimagetool https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x /tmp/appimagetool
ARCH=x86_64 /tmp/appimagetool "$APPDIR" dist/TWG-3D-x86_64.AppImage
```

> **Persyaratan PC Klien Linux:** Driver Vulkan aktif (`mesa-vulkan-drivers` untuk AMD/Intel atau driver NVIDIA) serta `xdg-desktop-portal` (untuk file dialog Admin).

---

## 2.4 Packaging untuk macOS (`.app` Bundle & `.dmg`)

Build macOS harus dijalankan di mesin **macOS** (atau menggunakan runner GitHub Actions `macos-14` / `macos-15`).

### Prasyarat Build di macOS

```bash
xcode-select --install
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
```

### 1. Build Apple Silicon, Intel, atau Universal Binary

```bash
# Apple Silicon (M1/M2/M3/M4)
cargo build --release -p twg-app --target aarch64-apple-darwin

# Intel Mac (opsional)
cargo build --release -p twg-app --target x86_64-apple-darwin

# Gabungkan menjadi Universal Binary (mendukung Apple Silicon + Intel sekaligus)
mkdir -p dist/macos
lipo -create \
  target/aarch64-apple-darwin/release/twg-app \
  target/x86_64-apple-darwin/release/twg-app \
  -output dist/macos/twg-app-universal
```

*(Jika hanya untuk Apple Silicon, cukup gunakan `target/aarch64-apple-darwin/release/twg-app` langsung.)*

### 2. Membuat `.app` Bundle (`TWG-3D.app`)

Di macOS, aplikasi dalam `/Applications/TWG-3D.app` bersifat **read-only**, sehingga kita pakai launcher script di `Contents/MacOS/twg-app` agar `offline_assets` dan `.tile_cache` disimpan di `~/Library/Application Support/TWG-3D/`.

```bash
APP="dist/macos/TWG-3D.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# Salin binary Rust
cp dist/macos/twg-app-universal "$APP/Contents/MacOS/twg-app-bin"
chmod +x "$APP/Contents/MacOS/twg-app-bin"

# Buat launcher script
cat <<'EOF' > "$APP/Contents/MacOS/twg-app"
#!/usr/bin/env bash
set -euo pipefail
MACOS_DIR="$(cd "$(dirname "$0")" && pwd)"
DATA_DIR="$HOME/Library/Application Support/TWG-3D"
export TWG_API_URL="${TWG_API_URL:-https://twg.domain-anda.com}"
export TWG_LOCAL_ASSETS_DIR="${TWG_LOCAL_ASSETS_DIR:-$DATA_DIR/offline_assets}"
mkdir -p "$DATA_DIR" "$TWG_LOCAL_ASSETS_DIR"
cd "$DATA_DIR"
exec "$MACOS_DIR/twg-app-bin" "$@"
EOF
chmod +x "$APP/Contents/MacOS/twg-app"

# Buat Info.plist
cat <<'EOF' > "$APP/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>TWG 3D</string>
  <key>CFBundleDisplayName</key>
  <string>Tactical Wall Game 3D</string>
  <key>CFBundleIdentifier</key>
  <string>id.twg.desktop</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleExecutable</key>
  <string>twg-app</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
</dict>
</plist>
EOF
```

### 3. Membuat `.dmg` Installer

```bash
mkdir -p dist/macos/dmg_root
cp -R "$APP" dist/macos/dmg_root/
ln -sf /Applications dist/macos/dmg_root/Applications

hdiutil create \
  -volname "Tactical Wall Game 3D" \
  -srcfolder dist/macos/dmg_root \
  -ov -format UDZO \
  dist/TWG-3D-macOS-universal.dmg
```

### 4. Code Signing & Notarization (macOS Gatekeeper)

- **Distribusi Publik (Apple Developer ID):**
  ```bash
  codesign --deep --force --options runtime --sign "Developer ID Application: NAMA_ANDA (TEAMID)" "$APP"
  codesign --force --sign "Developer ID Application: NAMA_ANDA (TEAMID)" dist/TWG-3D-macOS-universal.dmg
  xcrun notarytool submit dist/TWG-3D-macOS-universal.dmg --keychain-profile "NOTARY_PROFILE" --wait
  xcrun stapler staple dist/TWG-3D-macOS-universal.dmg
  ```
- **Distribusi Internal (Tanpa Akun Apple Developer):**
  Ad-hoc sign agar berjalan di Apple Silicon:
  ```bash
  codesign --deep --force --sign - "$APP"
  ```
  Di Mac penerima (jika muncul peringatan Gatekeeper saat membuka dari unduhan):
  ```bash
  xattr -dr com.apple.quarantine "/Applications/TWG-3D.app"
  ```

---

## 2.5 Otomatisasi Build Multi-OS (GitHub Actions Matrix)

Jika Anda tidak memiliki ketiga OS secara fisik, simpan workflow berikut di `.github/workflows/release-app.yml` untuk build **Windows + Linux + macOS** otomatis setiap kali membuat tag `v*`:

```yaml
name: Build & Package twg-app

on:
  push:
    tags: ["v*"]
  workflow_dispatch:

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: windows-latest
            artifact: twg-app-windows-x64
          - os: ubuntu-24.04
            artifact: twg-app-linux-x64
          - os: macos-14
            artifact: twg-app-macos-arm64

    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      - name: Install Linux Bevy dependencies
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y pkg-config libx11-dev libasound2-dev libudev-dev \
            libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libgtk-3-dev

      - name: Build twg-app (release)
        run: cargo build --release -p twg-app

      - name: Package Windows ZIP
        if: runner.os == 'Windows'
        shell: pwsh
        run: |
          New-Item -ItemType Directory -Force -Path dist | Out-Null
          Copy-Item target\release\twg-app.exe dist\twg-app.exe
          Compress-Archive -Path dist\* -DestinationPath ${{ matrix.artifact }}.zip -Force

      - name: Package Linux tar.gz
        if: runner.os == 'Linux'
        run: |
          mkdir -p dist/twg-app
          cp target/release/twg-app dist/twg-app/twg-app
          tar -C dist -czf ${{ matrix.artifact }}.tar.gz twg-app

      - name: Package macOS tar.gz
        if: runner.os == 'macOS'
        run: |
          mkdir -p dist/twg-app
          cp target/release/twg-app dist/twg-app/twg-app
          codesign --force --sign - dist/twg-app/twg-app
          tar -C dist -czf ${{ matrix.artifact }}.tar.gz twg-app

      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact }}
          path: |
            *.zip
            *.tar.gz
```

---

# Bagian 3 — Checklist Uji Pasca-Deploy & Troubleshooting

## Checklist Verifikasi
- [ ] `https://twg.domain-anda.com/healthz` & `/readyz` mengembalikan HTTP `200`.
- [ ] `https://twg.domain-anda.com/api/v1/assets/models/manifest` menampilkan `311` model & `864` file.
- [ ] Buka `twg-app` di PC baru → progress bar sync langsung berjalan dan mengunduh 864 file ke `offline_assets/`.
- [ ] Tutup & buka ulang `twg-app` → sync selesai < 1 detik (`0 baru, 0 update, 864 up-to-date`).
- [ ] Matikan internet / server → `twg-app` tetap membuka model 3D dari cache offline lokal.
- [ ] Login sebagai `admin` → halaman **Assets 3D** tampil, preview LOD0/LOD2 & animasi Walk berjalan, tambah/edit/hapus model sukses, lalu PC kedua otomatis menerima pembaruan saat dibuka.
- [ ] Login sebagai `operator` → tab **Assets 3D** tidak muncul & upload langsung ke API ditolak HTTP `403`.

## Troubleshooting Umum

| Gejala | Penyebab Umum | Solusi |
| --- | --- | --- |
| Upload model 3D besar gagal `HTTP 413` | Limit body default Nginx (`1M`) | Pastikan `client_max_body_size 130M;` ada di blok `server {}` Nginx lalu `sudo systemctl reload nginx`. |
| Upload Admin gagal konversi LOD2 / Draco | `node` atau `@gltf-transform/cli` tidak ditemukan oleh `systemd` | Pastikan `WorkingDirectory=/opt/twg-server`, `scripts/convert-single-model.mjs` ada, dan `which node` masuk dalam `Environment=PATH=...` di unit systemd. |
| Status `WS OFFLINE` di `twg-app` | Header WebSocket hilang di proxy | Pastikan blok `location /ws` di Nginx memiliki `proxy_http_version 1.1`, `Upgrade`, dan `Connection "upgrade"`. |
| `twg-app` gagal simpan cache di macOS / Program Files | Folder instalasi read-only | Jalankan lewat wrapper script yang menyetel `TWG_LOCAL_ASSETS_DIR` & `cd` ke direktori data user (`%LOCALAPPDATA%`, `~/.local/share`, atau `~/Library/Application Support`). |
| Dialog pilih file (Admin) tidak muncul di Linux | Portal GTK/XDG belum terpasang | Pasang `xdg-desktop-portal` + `xdg-desktop-portal-gtk` (atau `-kde`) di PC klien Linux. |
