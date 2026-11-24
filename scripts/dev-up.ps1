$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
Push-Location $Root
try {
    Write-Host "Starting Podman PostgreSQL container (twg-rust-postgres on port 5435)..." -ForegroundColor Cyan
    podman compose up -d
    if ($LASTEXITCODE -ne 0) {
        throw "podman compose up -d failed with exit code $LASTEXITCODE"
    }

    Write-Host "Waiting for PostgreSQL healthcheck..." -ForegroundColor Yellow
    $prevPref = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        for ($i = 0; $i -lt 30; $i++) {
            podman exec twg-rust-postgres pg_isready -U twg -d twg_rust 2>$null | Out-Null
            if ($LASTEXITCODE -eq 0) {
                Write-Host "PostgreSQL is healthy and ready on 127.0.0.1:5435 (db: twg_rust, user: twg)." -ForegroundColor Green
                exit 0
            }
            Start-Sleep -Seconds 1
        }
    }
    finally {
        $ErrorActionPreference = $prevPref
    }
    throw "PostgreSQL did not become healthy within 30 seconds."
}
finally {
    Pop-Location
}
