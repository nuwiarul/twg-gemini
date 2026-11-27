param(
    [string]$ServerUrl = "https://twg.domain-anda.com",
    [string]$Version = "0.1.0",
    [switch]$SkipBuild,
    [switch]$IncludeOfflineSeed,
    [string]$FfmpegBinDir = "",
    [switch]$DownloadFfmpeg
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
Push-Location $RepoRoot

try {
    if (-not $SkipBuild) {
        Write-Host "==> Building twg-app (release) for Windows x64..." -ForegroundColor Cyan
        cargo build --release -p twg-app
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }

    $ExePath = Join-Path $RepoRoot "target\release\twg-app.exe"
    if (-not (Test-Path $ExePath)) {
        throw "Release binary not found at $ExePath"
    }

    $DistRoot = Join-Path $RepoRoot "dist"
    $StageDir = Join-Path $DistRoot "twg-app-windows-x64"
    if (Test-Path $StageDir) {
        Remove-Item -Recurse -Force $StageDir
    }
    New-Item -ItemType Directory -Force -Path $StageDir | Out-Null

    Copy-Item $ExePath (Join-Path $StageDir "twg-app.exe")

    $IconSrc = Join-Path $RepoRoot "packaging\icons\twg-app.ico"
    if (Test-Path $IconSrc) {
        Copy-Item $IconSrc (Join-Path $StageDir "twg-app.ico")
    }

    # Bundle sidecar ffmpeg.exe & ffprobe.exe into bin/ so Videotron / video playback works offline without manual install
    $StageBin = Join-Path $StageDir "bin"
    $FfmpegSrc = ""
    $FfprobeSrc = ""
    if ($FfmpegBinDir -and (Test-Path (Join-Path $FfmpegBinDir "ffmpeg.exe"))) {
        $FfmpegSrc = Join-Path $FfmpegBinDir "ffmpeg.exe"
        $FfprobeSrc = Join-Path $FfmpegBinDir "ffprobe.exe"
    } elseif (-not $DownloadFfmpeg) {
        $ffCmd = Get-Command ffmpeg.exe -ErrorAction SilentlyContinue
        $fpCmd = Get-Command ffprobe.exe -ErrorAction SilentlyContinue
        if ($ffCmd -and $fpCmd) {
            $FfmpegSrc = $ffCmd.Source
            $FfprobeSrc = $fpCmd.Source
        }
    }
    if ((-not $FfmpegSrc -or -not (Test-Path $FfmpegSrc)) -and ($DownloadFfmpeg -or $env:GITHUB_ACTIONS -eq "true")) {
        $CacheDir = Join-Path $DistRoot "_ffmpeg_win_cache"
        $CacheFfmpeg = Join-Path $CacheDir "ffmpeg.exe"
        $CacheFfprobe = Join-Path $CacheDir "ffprobe.exe"
        if (-not ((Test-Path $CacheFfmpeg) -and (Test-Path $CacheFfprobe))) {
            Write-Host "==> Downloading FFmpeg essentials sidecar for Windows x64..." -ForegroundColor Cyan
            New-Item -ItemType Directory -Force -Path $CacheDir | Out-Null
            $FfZip = Join-Path $CacheDir "ffmpeg-release-essentials.zip"
            $FfExtract = Join-Path $CacheDir "extracted"
            if (Test-Path $FfExtract) { Remove-Item -Recurse -Force $FfExtract }
            Invoke-WebRequest -Uri "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip" -OutFile $FfZip
            Expand-Archive -Path $FfZip -DestinationPath $FfExtract -Force
            $FoundFf = Get-ChildItem $FfExtract -Recurse -Filter "ffmpeg.exe" | Select-Object -First 1
            $FoundFp = Get-ChildItem $FfExtract -Recurse -Filter "ffprobe.exe" | Select-Object -First 1
            if ($FoundFf -and $FoundFp) {
                Copy-Item $FoundFf.FullName $CacheFfmpeg -Force
                Copy-Item $FoundFp.FullName $CacheFfprobe -Force
            }
            Remove-Item -Recurse -Force $FfExtract -ErrorAction SilentlyContinue
            Remove-Item -Force $FfZip -ErrorAction SilentlyContinue
        }
        if ((Test-Path $CacheFfmpeg) -and (Test-Path $CacheFfprobe)) {
            $FfmpegSrc = $CacheFfmpeg
            $FfprobeSrc = $CacheFfprobe
        }
    }
    if ($FfmpegSrc -and $FfprobeSrc -and (Test-Path $FfmpegSrc) -and (Test-Path $FfprobeSrc)) {
        Write-Host "==> Bundling FFmpeg sidecar into bin/..." -ForegroundColor Cyan
        New-Item -ItemType Directory -Force -Path $StageBin | Out-Null
        Copy-Item $FfmpegSrc (Join-Path $StageBin "ffmpeg.exe") -Force
        Copy-Item $FfprobeSrc (Join-Path $StageBin "ffprobe.exe") -Force
    } else {
        Write-Host "==> FFmpeg sidecar not bundled (pass -DownloadFfmpeg or -FfmpegBinDir to embed)." -ForegroundColor Yellow
    }

    # Launcher batch script that configures writable per-user offline cache + server URL
    $LauncherBat = @"
@echo off
setlocal
if "%TWG_API_URL%"=="" set TWG_API_URL=$ServerUrl
set "TWG_DATA_DIR=%LOCALAPPDATA%\TWG-3D"
if "%TWG_LOCAL_ASSETS_DIR%"=="" set "TWG_LOCAL_ASSETS_DIR=%TWG_DATA_DIR%\offline_assets"
if "%TWG_TILE_CACHE_DIR%"=="" set "TWG_TILE_CACHE_DIR=%TWG_DATA_DIR%\tile_cache"
if not exist "%TWG_LOCAL_ASSETS_DIR%" mkdir "%TWG_LOCAL_ASSETS_DIR%"
if not exist "%TWG_TILE_CACHE_DIR%" mkdir "%TWG_TILE_CACHE_DIR%"
cd /d "%TWG_DATA_DIR%"
start "" "%~dp0twg-app.exe" %*
"@
    Set-Content -Encoding ASCII -Path (Join-Path $StageDir "TWG-3D.bat") -Value $LauncherBat

    if ($IncludeOfflineSeed) {
        $SeedSrc = Join-Path $RepoRoot "client_assets"
        if (-not (Test-Path $SeedSrc)) {
            $SeedSrc = Join-Path $RepoRoot "assets"
        }
        if (Test-Path (Join-Path $SeedSrc "models")) {
            Write-Host "==> Including offline 3D model seed from $SeedSrc..." -ForegroundColor Yellow
            $SeedDst = Join-Path $StageDir "offline_assets"
            New-Item -ItemType Directory -Force -Path $SeedDst | Out-Null
            Copy-Item -Recurse -Force (Join-Path $SeedSrc "models") (Join-Path $SeedDst "models")
            if (Test-Path (Join-Path $SeedSrc "catalog")) {
                Copy-Item -Recurse -Force (Join-Path $SeedSrc "catalog") (Join-Path $SeedDst "catalog")
            }
            if (Test-Path (Join-Path $SeedSrc "sync_manifest.json")) {
                Copy-Item -Force (Join-Path $SeedSrc "sync_manifest.json") (Join-Path $SeedDst "sync_manifest.json")
            }
        }
    }

    $ReadmeTxt = @"
Tactical Wall Game 3D (twg-app) v$Version — Windows x64
=======================================================

Cara Menjalankan:
1. Klik dua kali TWG-3D.bat (atau jalankan twg-app.exe langsung).
2. Saat pertama kali dibuka, aplikasi otomatis menghubungi server ($ServerUrl)
   dan menyinkronkan seluruh model 3D glTF (LOD0 + LOD2) ke cache lokal:
   %LOCALAPPDATA%\TWG-3D\offline_assets
3. Untuk mengubah URL server, edit langsung kolom Server URL pada layar Login
   atau ubah variabel TWG_API_URL di dalam TWG-3D.bat.

Persyaratan:
- Windows 10/11 64-bit dengan GPU support Vulkan atau DirectX 12.
"@
    Set-Content -Encoding UTF8 -Path (Join-Path $StageDir "README.txt") -Value $ReadmeTxt

    $ZipPath = Join-Path $DistRoot "twg-app-windows-x64-v$Version.zip"
    Write-Host "==> Creating portable archive: $ZipPath" -ForegroundColor Cyan
    Compress-Archive -Path (Join-Path $StageDir "*") -DestinationPath $ZipPath -Force

    # Build Inno Setup installer if ISCC.exe is installed
    $IsccCandidates = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe"
    )
    $Iscc = $IsccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if ($Iscc) {
        Write-Host "==> Inno Setup detected ($Iscc) — building Windows Setup .exe..." -ForegroundColor Cyan
        & $Iscc "/DMyAppVersion=$Version" (Join-Path $PSScriptRoot "twg-app.iss")
        if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
    } else {
        Write-Host "==> Inno Setup not installed — skipped .exe installer (ZIP archive ready)." -ForegroundColor Yellow
    }

    Write-Host "`nDone! Output artifacts in $DistRoot" -ForegroundColor Green
} finally {
    Pop-Location
}
