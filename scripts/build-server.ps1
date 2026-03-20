# build-server.ps1 - Build wifikey-server Tauri app
# Usage: .\scripts\build-server.ps1 [-Release]

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$ServerDir = Join-Path $ProjectRoot "wifikey-server"

# パス長制限回避
$env:CARGO_TARGET_DIR = "C:\espbuild"

if ($Release) {
    Write-Host "=== Building wifikey-server (release + installer) ===" -ForegroundColor Cyan
    Push-Location $ServerDir
    try {
        npx tauri build
        if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
    } finally {
        Pop-Location
    }

    $distDir = Join-Path $ProjectRoot "dist"
    New-Item -ItemType Directory -Force -Path $distDir | Out-Null

    $bundleBase = Join-Path "C:\espbuild" "release\bundle"
    $copied = @()

    $nsis = Get-ChildItem (Join-Path $bundleBase "nsis") -Filter "*.exe" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($nsis) { Copy-Item $nsis.FullName -Destination $distDir -Force; $copied += $nsis.Name }

    $msi = Get-ChildItem (Join-Path $bundleBase "msi") -Filter "*.msi" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($msi)  { Copy-Item $msi.FullName  -Destination $distDir -Force; $copied += $msi.Name  }

    Write-Host ""
    Write-Host "=== Build complete ===" -ForegroundColor Green
    $copied | ForEach-Object { Write-Host "  dist\$_" -ForegroundColor Yellow }
    if ($copied.Count -eq 0) { throw "No installer found in $bundleBase" }
} else {
    Write-Host "=== Building wifikey-server (debug) ===" -ForegroundColor Cyan
    Push-Location $ServerDir
    try {
        npx tauri build --debug
        if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
    } finally {
        Pop-Location
    }
    Write-Host "=== Build complete ===" -ForegroundColor Green
}
