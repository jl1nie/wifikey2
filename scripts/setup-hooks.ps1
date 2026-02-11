# Setup git hooks for development (Windows)

$ScriptDir = $PSScriptRoot
$ProjectRoot = Split-Path -Parent $ScriptDir

Write-Host "Installing git hooks..."

Copy-Item "$ScriptDir\pre-commit" "$ProjectRoot\.git\hooks\pre-commit" -Force

Write-Host "Git hooks installed successfully!"
Write-Host ""
Write-Host "Pre-commit hook will check:"
Write-Host "  - cargo fmt for all crates"
Write-Host "  - cargo clippy for PC crates (wifikey-server, wksocket, mqttstunclient)"
Write-Host "  - cargo clippy for ESP32 crate (wifikey) if esp toolchain available"
