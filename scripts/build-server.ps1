# build-server.ps1 - Build wifikey-server Tauri app
# Usage: .\scripts\build-server.ps1 [-Release]

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$ServerDir = Join-Path $ProjectRoot "wifikey-server"

if ($Release) {
    Write-Host "=== Building wifikey-server (release + installer) ===" -ForegroundColor Cyan
    Push-Location $ServerDir
    try {
        npx tauri build
        if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
    } finally {
        Pop-Location
    }

    $nsis = Join-Path $ProjectRoot "target\release\bundle\nsis"
    $msi  = Join-Path $ProjectRoot "target\release\bundle\msi"
    Write-Host ""
    Write-Host "=== Build complete ===" -ForegroundColor Green
    Write-Host "  NSIS: $(Get-ChildItem $nsis -Filter *.exe | Select-Object -First 1)"
    Write-Host "  MSI:  $(Get-ChildItem $msi  -Filter *.msi | Select-Object -First 1)"
} else {
    Write-Host "=== Building wifikey-server (dev) ===" -ForegroundColor Cyan
    Push-Location $ServerDir
    try {
        npx tauri build --debug
        if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
    } finally {
        Pop-Location
    }
    Write-Host "=== Build complete ===" -ForegroundColor Green
}
