#!/usr/bin/env bash
set -euo pipefail

# --- Config ---
CLI_DIR="clients/cli"
BIN_NAME="nexus-cli"   # change this to your binary name

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
AUTO_INSTALL=false

# Parse arguments
for arg in "$@"; do
    case $arg in
        --install|-i)
            AUTO_INSTALL=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [--install|-i]"
            echo
            echo "Options:"
            echo "  --install, -i   Automatically install system-wide without asking"
            echo "  --help, -h      Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $arg"
            exit 1
            ;;
    esac
done

# Run build
build_cli

# Decide install mode
if $AUTO_INSTALL; then
    install_cli
else
    read -rp "Do you want to install $BIN_NAME system-wide? [y/N] " yn
    case $yn in
        [Yy]* ) install_cli ;;
        * ) echo "Skipping installation. You can run it from $CLI_DIR/target/release/$BIN_NAME";;
    esac
fi
echo "🎉 All done!"