param(
    [string]$Brand = "",
    [Alias("ServerUrl")]
    [string]$PublicServerUrl = "",
    [string]$BrandDir = "",
    [string]$LogosDir = "",
    [string]$OrgName = "",
    [string]$OrgSubtitle = "",
    [string]$AppTitle = "",
    [string]$Version = "0.1.0",
    [switch]$SkipBuild,
    [switch]$IncludeOfflineSeed,
    [string]$FfmpegBinDir = "",
    [switch]$DownloadFfmpeg
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
Push-Location $RepoRoot

$SavedBrandEnv = @{
    TWG_BRAND             = $env:TWG_BRAND
    TWG_BRAND_DIR         = $env:TWG_BRAND_DIR
    TWG_BRAND_LOGOS_DIR   = $env:TWG_BRAND_LOGOS_DIR
    TWG_PUBLIC_SERVER_URL = $env:TWG_PUBLIC_SERVER_URL
    TWG_ORG_NAME          = $env:TWG_ORG_NAME
    TWG_ORG_SUBTITLE      = $env:TWG_ORG_SUBTITLE
    TWG_APP_TITLE         = $env:TWG_APP_TITLE
}

try {
    if ($Brand)           { $env:TWG_BRAND = $Brand }
    if ($BrandDir)        { $env:TWG_BRAND_DIR = $BrandDir }
    if ($LogosDir)        { $env:TWG_BRAND_LOGOS_DIR = $LogosDir }
    if ($PublicServerUrl) { $env:TWG_PUBLIC_SERVER_URL = $PublicServerUrl }
    if ($OrgName)         { $env:TWG_ORG_NAME = $OrgName }
    if ($OrgSubtitle)     { $env:TWG_ORG_SUBTITLE = $OrgSubtitle }
    if ($AppTitle)        { $env:TWG_APP_TITLE = $AppTitle }

    $ResolvedBrandDir = if ($env:TWG_BRAND_DIR) {
        if ([System.IO.Path]::IsPathRooted($env:TWG_BRAND_DIR)) { $env:TWG_BRAND_DIR } else { Join-Path $RepoRoot $env:TWG_BRAND_DIR }
    } elseif ($env:TWG_BRAND) {
        Join-Path $RepoRoot "packaging\brands\$($env:TWG_BRAND)"
    } else {
        Join-Path $RepoRoot "packaging\brands\polda-bali"
    }
    if (-not (Test-Path $ResolvedBrandDir)) {
        throw "Brand profile folder not found: $ResolvedBrandDir (contoh: -Brand polda-bali atau -Brand polda-jateng)"
    }

    $ResolvedLogosDir = if ($env:TWG_BRAND_LOGOS_DIR) {
        if ([System.IO.Path]::IsPathRooted($env:TWG_BRAND_LOGOS_DIR)) { $env:TWG_BRAND_LOGOS_DIR } else { Join-Path $RepoRoot $env:TWG_BRAND_LOGOS_DIR }
    } else {
        Join-Path $ResolvedBrandDir "logos"
    }
    $FoundLogos = @(1, 2, 3 | Where-Object { Test-Path (Join-Path $ResolvedLogosDir "$_.png") } | ForEach-Object { "$_.png" })

    Write-Host "==> Branding Profile: $ResolvedBrandDir" -ForegroundColor Cyan
    Write-Host "    Logos embedded  : $(if ($FoundLogos.Count -gt 0) { $FoundLogos -join ', ' } else { '(none)' })" -ForegroundColor Cyan
    Write-Host "    Public Server   : $(if ($env:TWG_PUBLIC_SERVER_URL) { 'override via param (compiled into binary, hidden)' } else { 'from brand.env (compiled into binary, hidden)' })" -ForegroundColor Cyan

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
1. Klik dua kali twg-app.exe langsung (tidak memerlukan file .bat).
2. Saat pertama kali dibuka, aplikasi otomatis terhubung ke Server Publik bawaan
   dan menyinkronkan model 3D glTF (LOD0 + LOD2) ke folder data lokal:
   %LOCALAPPDATA%\TWG-3D\offline_assets
3. Untuk beralih antara mode Server Publik dan Server Lokal (LAN), buka
   menu "Pengaturan server" pada bagian bawah halaman Login.
4. (Opsional) Jika membutuhkan override environment khusus, letakkan file
   twg-app.env (atau .env) di folder yang sama dengan twg-app.exe atau di
   %LOCALAPPDATA%\TWG-3D\twg-app.env.

Persyaratan:
- Windows 10/11 64-bit dengan GPU support Vulkan atau DirectX 12.
"@
    Set-Content -Encoding UTF8 -Path (Join-Path $StageDir "README.txt") -Value $ReadmeTxt

    $BrandSlug = Split-Path -Leaf $ResolvedBrandDir
    $ZipPath = Join-Path $DistRoot "twg-app-$BrandSlug-windows-x64-v$Version.zip"
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
        & $Iscc "/DMyAppVersion=$Version" "/DMyBrandSlug=$BrandSlug" (Join-Path $PSScriptRoot "twg-app.iss")
        if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
    } else {
        Write-Host "==> Inno Setup not installed — skipped .exe installer (ZIP archive ready)." -ForegroundColor Yellow
    }

    Write-Host "`nDone! Output artifacts in $DistRoot" -ForegroundColor Green
} finally {
    foreach ($k in $SavedBrandEnv.Keys) {
        if ($null -eq $SavedBrandEnv[$k]) {
            Remove-Item "Env:$k" -ErrorAction SilentlyContinue
        } else {
            Set-Item "Env:$k" $SavedBrandEnv[$k]
        }
    }
    Pop-Location
}
