#!/bin/bash

# Safe Pocket Installation Script

set -e

echo "🔧 Building Safe Pocket..."
cargo build --release

BINARY="./target/release/spocket"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

if [ ! -f "$BINARY" ]; then
    echo "❌ Build failed - binary not found"
    exit 1
fi

# Create install directory if it doesn't exist
mkdir -p "$INSTALL_DIR"

# Copy binary
echo "📦 Installing to $INSTALL_DIR/spocket..."
cp "$BINARY" "$INSTALL_DIR/spocket"
chmod +x "$INSTALL_DIR/spocket"

# Check if directory is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "⚠️  $INSTALL_DIR is not in your PATH"
    echo ""
    echo "Add this line to your shell config (~/.bashrc, ~/.zshrc, etc.):"
    echo ""
    echo "    export PATH=\"\$PATH:$INSTALL_DIR\""
    echo ""
fi

echo "✅ Installation complete!"
echo ""
echo "Try it out:"
echo "  spocket --help"
echo "  spocket register myproject=\"\$(pwd)\""
echo "  spocket -i myproject"
