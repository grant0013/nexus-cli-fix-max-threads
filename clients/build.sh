#!/usr/bin/env bash
set -euo pipefail

# --- Config ---
CLI_DIR="clients/cli"
BIN_NAME="nexus-cli"   # change to match your binary name

# --- Functions ---
build_cli() {
    echo "🚀 Building $BIN_NAME..."
    cd "$CLI_DIR"
    cargo build --release
    cd - >/dev/null
    echo "✅ Build finished."
}

install_cli() {
    echo "📦 Installing $BIN_NAME system-wide..."
    sudo install -m 755 "$CLI_DIR/target/release/$BIN_NAME" /usr/local/bin/$BIN_NAME
    echo "✅ Installed to /usr/local/bin/$BIN_NAME"
}

# --- Main ---
build_cli

read -rp "Do you want to install $BIN_NAME system-wide? [y/N] " yn
case $yn in
    [Yy]* ) install_cli ;;
    * ) echo "Skipping installation. You can run it from $CLI_DIR/target/release/$BIN_NAME";;
esac
