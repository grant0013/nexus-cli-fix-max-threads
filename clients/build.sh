#!/usr/bin/env bash
set -euo pipefail

# --- Config ---
CLI_DIR="clients/cli"
BIN_NAME="nexus-cli"   # change this to your binary name

# --- Functions ---
check_rust() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "⚠ Rust and Cargo are not installed."
        read -rp "Do you want to install Rust via rustup? [Y/n] " install_rust
        case $install_rust in
            [Nn]* ) 
                echo "Rust is required to build $BIN_NAME. Exiting."
                exit 1
                ;;
            * )
                curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
                echo "✅ Rust installed. Please restart your terminal or run 'source \$HOME/.cargo/env'."
                exit 0
                ;;
        esac
    fi
}

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

# Check Rust & Cargo
check_rust

# Build CLI
build_cli

# Install if requested
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