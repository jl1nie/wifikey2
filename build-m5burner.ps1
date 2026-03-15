# build-m5burner.ps1 - Build M5Burner-compatible firmware for WiFiKey2
#
# Produces a firmware that auto-enters AP mode on first boot when no profiles are configured.
# After flashing via M5Burner, connect to "WifiKey-XXXXXX" (password: wifikey2) and open
# http://192.168.71.1 to configure WiFi and server settings.
#
# Usage: .\build-m5burner.ps1 [-Board m5atom_lite|esp32_wrover] [-OutDir .]
#
# Output: wifikey2-<board>-<version>.zip (M5Burner compatible)

param(
    [ValidateSet("m5atom_lite", "esp32_wrover")]
    [string]$Board = "m5atom_lite",

    [string]$OutDir = $PSScriptRoot
)

$ErrorActionPreference = "Stop"

Write-Host "=== wifikey2 M5Burner firmware build ===" -ForegroundColor Cyan
Write-Host "  Board: $Board"

# Source ESP environment
$exportScript = Join-Path $env:USERPROFILE "export-esp.ps1"
if (Test-Path $exportScript) {
    . $exportScript
} else {
    Write-Host "[ERROR] $exportScript not found. Run .\setup.ps1 first." -ForegroundColor Red
    exit 1
}

# Use short target directory to avoid ESP-IDF path length limitation on Windows
$env:CARGO_TARGET_DIR = "C:\espbuild"

# Select board feature
$boardFeature = switch ($Board) {
    "esp32_wrover" { "board_esp32_wrover" }
    default        { "board_m5atom" }  # m5atom_lite (GPIO27=LED, GPIO39=BTN)
}

# Read firmware version from wifikey/Cargo.toml
$cargoToml = Get-Content (Join-Path $PSScriptRoot "wifikey\Cargo.toml") -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
    $version = $Matches[1]
} else {
    $version = "0.0.0"
}
Write-Host "  Version: $version"
Write-Host "  Target:  $env:CARGO_TARGET_DIR"
Write-Host ""

# ----------------------------------------------------------------
# Build (release) without cfg.toml profile data
# build.rs emits empty CFG_WIFI_SSID / CFG_SERVER_NAME when cfg.toml
# has no [wifikey] section or is absent → firmware auto-enters AP mode
# when NVS contains no profiles.
# ----------------------------------------------------------------
# Temporarily rename cfg.toml if it exists so build.rs sees empty defaults
$cfgPath = Join-Path $PSScriptRoot "cfg.toml"
$cfgBackup = Join-Path $PSScriptRoot "cfg.toml.m5burner.bak"
$cfgRenamed = $false
if (Test-Path $cfgPath) {
    Rename-Item $cfgPath $cfgBackup
    $cfgRenamed = $true
    Write-Host "  [INFO] cfg.toml temporarily renamed for build" -ForegroundColor Yellow
}

Write-Host "--- Building (release) ---" -ForegroundColor Cyan
$wifikeyDir = Join-Path $PSScriptRoot "wifikey"
$buildArgs = @("build", "--release", "--no-default-features", "--features", "std,esp-idf-svc/native,$boardFeature")
Push-Location $wifikeyDir
try {
    cargo @buildArgs
    $buildOk = ($LASTEXITCODE -eq 0)
} finally {
    Pop-Location
    # Restore cfg.toml
    if ($cfgRenamed) {
        Rename-Item $cfgBackup $cfgPath
        Write-Host "  [INFO] cfg.toml restored" -ForegroundColor Yellow
    }
}

if (-not $buildOk) {
    Write-Host "[ERROR] Build failed." -ForegroundColor Red
    exit 1
}
Write-Host "[OK] Build succeeded." -ForegroundColor Green
Write-Host ""

# ----------------------------------------------------------------
# Generate merged firmware image (no NVS patch → empty NVS → AP mode)
# ----------------------------------------------------------------
$binary = Join-Path $env:CARGO_TARGET_DIR "xtensa-esp32-espidf\release\wifikey"
$binName = "wifikey2-$Board-$version.bin"
$firmwareBin = Join-Path $env:CARGO_TARGET_DIR $binName

Write-Host "--- Generating merged firmware image ---" -ForegroundColor Cyan
& espflash save-image --chip esp32 --merge $binary $firmwareBin
if ($LASTEXITCODE -ne 0) {
    Write-Host "[ERROR] Firmware image generation failed." -ForegroundColor Red
    exit 1
}
Write-Host "[OK] Firmware image: $firmwareBin" -ForegroundColor Green
Write-Host ""

# ----------------------------------------------------------------
# Create M5Burner-compatible zip package
# ----------------------------------------------------------------
Write-Host "--- Creating M5Burner package ---" -ForegroundColor Cyan

$zipName = "wifikey2-$Board-$version.zip"
$zipPath = Join-Path $OutDir $zipName

# M5Burner config.json
$configJson = @{
    version  = $version
    download = ""
    describe = "WiFiKey2 Remote CW Keyer ($Board) v$version. Auto-enters AP mode on first boot. Connect to WifiKey-XXXXXX (password: wifikey2), then open http://192.168.71.1"
    language = "en"
    platform = "esp32"
    app      = @(
        @{ addr = "0x0"; bin = $binName }
    )
} | ConvertTo-Json -Depth 5

# Build zip in memory using .NET
Add-Type -Assembly System.IO.Compression.FileSystem
if (Test-Path $zipPath) { Remove-Item $zipPath }

$zipStream = [System.IO.File]::Open($zipPath, [System.IO.FileMode]::Create)
$archive = [System.IO.Compression.ZipArchive]::new($zipStream, [System.IO.Compression.ZipArchiveMode]::Create)

# Add config.json
$configEntry = $archive.CreateEntry("config.json")
$configWriter = [System.IO.StreamWriter]::new($configEntry.Open())
$configWriter.Write($configJson)
$configWriter.Close()

# Add firmware binary
$firmwareEntry = $archive.CreateEntry($binName)
$firmwareStream = $firmwareEntry.Open()
$fwBytes = [System.IO.File]::ReadAllBytes($firmwareBin)
$firmwareStream.Write($fwBytes, 0, $fwBytes.Length)
$firmwareStream.Close()

$archive.Dispose()
$zipStream.Close()

Write-Host "[OK] M5Burner package: $zipPath" -ForegroundColor Green
Write-Host ""
Write-Host "=== Done ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "Flash with M5Burner, then:" -ForegroundColor White
Write-Host "  1. Power on the device"
Write-Host "  2. Connect to WiFi AP: WifiKey-XXXXXX  (password: wifikey2)"
Write-Host "  3. Open http://192.168.71.1 in browser"
Write-Host "  4. Add WiFi profiles and server settings"
Write-Host "  5. Restart the device to connect"
