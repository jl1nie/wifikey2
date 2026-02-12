#!/bin/bash
# Setup git hooks for development

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "Installing git hooks..."

# Install pre-commit hook
cp "$SCRIPT_DIR/pre-commit" "$PROJECT_ROOT/.git/hooks/pre-commit"
chmod +x "$PROJECT_ROOT/.git/hooks/pre-commit"

echo "Git hooks installed successfully!"
echo ""
echo "Pre-commit hook will check:"
echo "  - cargo fmt for all crates"
echo "  - cargo clippy for PC crates (wifikey-server, wksocket, mqttstunclient)"
echo "  - cargo clippy for ESP32 crate (wifikey) if ~/export-esp.sh exists"
