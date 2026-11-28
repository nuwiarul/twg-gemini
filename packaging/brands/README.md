# Panduan Branding & Distribusi Multi-Polda (`packaging/brands`)

Simpan setiap profil instansi/Polda dalam sub-folder sendiri di dalam `packaging/brands/` tanpa pernah mengedit kode Rust (`.rs`).

## Struktur Folder
```text
packaging/brands/
├── polda-bali/
│   ├── brand.env          # PUBLIC_SERVER_URL, ORG_NAME, ORG_SUBTITLE, APP_TITLE
│   └── logos/
│       ├── 1.png          # Logo paling kiri
│       ├── 2.png          # Logo tengah (opsional)
│       └── 3.png          # Logo paling kanan (opsional)
└── polda-jateng/          # Contoh profil Polda lain (cukup duplikat folder polda-bali)
    ├── brand.env
    └── logos/
        ├── 1.png
        └── 2.png
```

> **Jumlah logo fleksibel (0–3):** Aplikasi membaca `1.png`, `2.png`, `3.png` dari kiri ke kanan.
> Jika hanya ada `1.png` → tampil 1 logo; `1.png` + `2.png` → 2 logo; ketiganya → 3 logo.

## Cara Packaging Multi-Platform (Langsung Binary, Tanpa `.bat`/`.sh`)

### Windows (`twg-app.exe` + ZIP + Setup `.exe`)
```powershell
# Pakai profil polda-bali:
.\packaging\windows\package-windows.ps1 -Brand polda-bali -PublicServerUrl "https://twg.poldabali.example.go.id" -Version 1.0.0

# Pakai profil polda-jateng:
.\packaging\windows\package-windows.ps1 -Brand polda-jateng -PublicServerUrl "https://twg.jateng.example.go.id" -Version 1.0.0
```

### Linux (`twg-app` + `.tar.gz` + `.deb` + `.AppImage`)
```bash
bash packaging/linux/package-linux.sh --brand polda-bali --public-server-url "https://twg.poldabali.example.go.id" --version 1.0.0
bash packaging/linux/package-linux.sh --brand polda-jateng --public-server-url "https://twg.jateng.example.go.id" --version 1.0.0
```

### macOS (`TWG-3D.app` + `.dmg`)
```bash
bash packaging/macos/package-macos.sh --brand polda-bali --arch universal --public-server-url "https://twg.poldabali.example.go.id" --version 1.0.0
bash packaging/macos/package-macos.sh --brand polda-jateng --arch universal --public-server-url "https://twg.jateng.example.go.id" --version 1.0.0
```

## Pembacaan Environment Opsional di Komputer Target (Tanpa Wrapper Script)
Binary `twg-app` langsung menentukan folder data & cache yang dapat ditulis per-user secara otomatis:
- **Windows**: `%LOCALAPPDATA%\TWG-3D\` (`offline_assets`, `tile_cache`, `server_settings.json`)
- **macOS**: `~/Library/Application Support/TWG-3D/`
- **Linux**: `~/.local/share/twg-app/`

Jika sewaktu-waktu dibutuhkan override environment di komputer target (misalnya lokasi cache/aset khusus), cukup letakkan file **`twg-app.env`** (atau **`.env`**) di **folder yang sama dengan binary `twg-app`**, di `TWG-3D.app/Contents/Resources/` (macOS), atau di folder data aplikasi user di atas.
