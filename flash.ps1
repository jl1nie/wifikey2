# flash.ps1 - Build and flash wifikey2 ESP32 firmware
# Usage: .\flash.ps1 [-Board m5atom|esp32_wrover] [-Port COM3] [-Release] [-MonitorOnly]

param(
    [ValidateSet("m5atom", "esp32_wrover")]
    [string]$Board = "m5atom",

    [string]$Port = "",

    [switch]$Release,

    [switch]$MonitorOnly
)

$ErrorActionPreference = "Stop"

Write-Host "=== wifikey2 build & flash ===" -ForegroundColor Cyan
Write-Host "  Board: $Board"

# Source ESP environment
$exportScript = Join-Path $env:USERPROFILE "export-esp.ps1"
if (Test-Path $exportScript) {
    . $exportScript
} else {
    Write-Host "[ERROR] $exportScript not found. Run .\setup.ps1 first." -ForegroundColor Red
    exit 1
}

# Check cfg.toml
$cfgPath = Join-Path $PSScriptRoot "cfg.toml"
if (-not (Test-Path $cfgPath)) {
    Write-Host "[ERROR] cfg.toml not found. Copy cfg-sample.toml to cfg.toml and edit it." -ForegroundColor Red
    exit 1
}

# Use short target directory to avoid ESP-IDF path length limitation on Windows
$env:CARGO_TARGET_DIR = "C:\espbuild"

# Build espflash port argument
$portArgs = @()
if ($Port -ne "") {
    $portArgs = @("-p", $Port)
    Write-Host "  Port:  $Port"
}

# Monitor only (no build/flash)
if ($MonitorOnly) {
    Write-Host ""
    Write-Host "--- Opening serial monitor ---" -ForegroundColor Cyan
    & espflash monitor @portArgs
    exit 0
}

# Select features based on board
$features = if ($Board -eq "esp32_wrover") {
    "board_esp32_wrover"
} else {
    "board_m5atom"
}

$profile = if ($Release) { "release" } else { "dev" }
$profileDir = if ($Release) { "release" } else { "debug" }

Write-Host "  Target: $env:CARGO_TARGET_DIR"
Write-Host "  Profile: $profile"
Write-Host ""

# Build from wifikey/ directory so .cargo/config.toml is picked up
# (contains target=xtensa-esp32-espidf, env vars MCU, ESP_IDF_VERSION, etc.)
Write-Host "--- Building ---" -ForegroundColor Cyan
$wifikeyDir = Join-Path $PSScriptRoot "wifikey"
$buildArgs = @("build", "--no-default-features", "--features", "std,esp-idf-svc/native,$features")
if ($Release) {
    $buildArgs += "--release"
}
Push-Location $wifikeyDir
try {
    cargo @buildArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[ERROR] Build failed." -ForegroundColor Red
        exit 1
    }
} finally {
    Pop-Location
}
Write-Host "[OK] Build succeeded." -ForegroundColor Green
Write-Host ""

# Generate NVS partition from cfg.toml
Write-Host "--- Generating NVS partition ---" -ForegroundColor Cyan
$pythonExe = "C:\.embuild\espressif\python_env\idf5.2_py3.13_env\Scripts\python.exe"
$nvsGenScript = "C:\.embuild\espressif\esp-idf\v5.2.2\components\nvs_flash\nvs_partition_generator\nvs_partition_gen.py"
$nvsCsv = Join-Path $PSScriptRoot "nvs_data.csv"
$nvsBin = Join-Path $PSScriptRoot "nvs.bin"

# Parse cfg.toml [wifikey] section and generate NVS CSV
$cfgContent = Get-Content $cfgPath -Raw
$inSection = $false
$nvsValues = @{}
foreach ($line in ($cfgContent -split "`n")) {
    $line = $line.Trim()
    if ($line -match '^\[wifikey\]') {
        $inSection = $true
        continue
    }
    if ($line -match '^\[' -and $inSection) {
        break
    }
    if ($inSection -and $line -match '^(\w+)\s*=\s*"(.+)"') {
        $nvsValues[$Matches[1]] = $Matches[2]
    }
    if ($inSection -and $line -match '^(\w+)\s*=\s*(\d+)$') {
        $nvsValues[$Matches[1]] = $Matches[2]
    }
}

if ($nvsValues.Count -eq 0) {
    Write-Host "[ERROR] No [wifikey] section found in cfg.toml" -ForegroundColor Red
    exit 1
}

# Encode WiFi profile as binary blob matching ConfigManager::WifiProfile::to_bytes()
# Format: [ssid_len][ssid][pass_len][pass][sname_len][sname][spass_len][spass]
function Encode-WifiProfile($ssid, $password, $serverName, $serverPassword) {
    $buf = [System.Collections.Generic.List[byte]]::new()
    foreach ($s in @($ssid, $password, $serverName, $serverPassword)) {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($s)
        $buf.Add([byte]$bytes.Length)
        $buf.AddRange($bytes)
    }
    return [Convert]::ToBase64String($buf.ToArray())
}

$ssid   = $nvsValues["wifi_ssid"]
$passwd = $nvsValues["wifi_passwd"]
$sname  = $nvsValues["server_name"]
$spass  = $nvsValues["server_password"]

if (-not $ssid -or -not $sname) {
    Write-Host "[ERROR] wifi_ssid and server_name are required in cfg.toml" -ForegroundColor Red
    exit 1
}

$profileBlob = Encode-WifiProfile $ssid $passwd $sname $spass

# Write NVS CSV (ConfigManager format: count + prof0 blob)
$csvLines = @(
    "key,type,encoding,value"
    "wifikey,namespace,,"
    "count,data,u8,1"
    "prof0,data,base64,$profileBlob"
)
$csvLines -join "`n" | Set-Content -NoNewline -Path $nvsCsv -Encoding UTF8
Write-Host "  Profile: ssid=$ssid server=$sname"
Write-Host "  NVS CSV written to $nvsCsv"

# Generate NVS binary (size=0x6000 = 24K)
& $pythonExe $nvsGenScript generate $nvsCsv $nvsBin 0x6000
if ($LASTEXITCODE -ne 0) {
    Write-Host "[ERROR] NVS partition generation failed." -ForegroundColor Red
    exit 1
}
Write-Host "[OK] NVS binary generated." -ForegroundColor Green
Write-Host ""

# Flash firmware
$binary = Join-Path $env:CARGO_TARGET_DIR "xtensa-esp32-espidf\$profileDir\wifikey"
Write-Host "--- Flashing firmware ---" -ForegroundColor Cyan
& espflash flash -a no-reset @portArgs $binary
if ($LASTEXITCODE -ne 0) {
    Write-Host "[ERROR] Firmware flash failed." -ForegroundColor Red
    exit 1
}
Write-Host "[OK] Firmware flashed (no reset)." -ForegroundColor Green

# Write NVS partition at offset 0x9000 (chip is still in bootloader)
Write-Host "--- Writing NVS partition ---" -ForegroundColor Cyan
& espflash write-bin -b no-reset @portArgs 0x9000 $nvsBin
if ($LASTEXITCODE -ne 0) {
    Write-Host "[ERROR] NVS partition write failed." -ForegroundColor Red
    exit 1
}
Write-Host "[OK] NVS partition written." -ForegroundColor Green
Write-Host ""

# Monitor
Write-Host "--- Opening serial monitor ---" -ForegroundColor Cyan
& espflash monitor @portArgs
