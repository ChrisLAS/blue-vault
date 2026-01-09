#!/bin/bash

# BlueVault Build Script
# This script helps build BlueVault for distribution

set -e

echo "🔨 BlueVault Build Script"
echo "=========================="

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust/Cargo not found. Please install Rust:"
    echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check for required system dependencies
echo "📋 Checking system dependencies..."

MISSING_DEPS=()

check_dep() {
    if ! command -v "$1" &> /dev/null; then
        MISSING_DEPS+=("$1")
    fi
}

# Required dependencies
check_dep "xorriso"
check_dep "growisofs"
check_dep "sha256sum"

# Optional dependencies
OPTIONAL_MISSING=()
if ! command -v "qrencode" &> /dev/null; then
    OPTIONAL_MISSING+=("qrencode")
fi
if ! command -v "rsync" &> /dev/null; then
    OPTIONAL_MISSING+=("rsync")
fi

if [ ${#MISSING_DEPS[@]} -gt 0 ]; then
    echo "❌ Missing required dependencies: ${MISSING_DEPS[*]}"
    echo ""
    echo "Install on Ubuntu/Debian:"
    echo "  sudo apt-get update"
    echo "  sudo apt-get install xorriso growisofs"
    echo ""
    echo "Install on Fedora/RHEL:"
    echo "  sudo dnf install xorriso growisofs"
    echo ""
    echo "Install on macOS:"
    echo "  brew install xorriso growisofs"
    echo ""
    exit 1
fi

if [ ${#OPTIONAL_MISSING[@]} -gt 0 ]; then
    echo "⚠️  Optional dependencies missing: ${OPTIONAL_MISSING[*]}"
    echo "   These provide additional features but are not required."
    echo ""
fi

# Get version from Cargo.toml
VERSION=$(grep '^version =' Cargo.toml | head -1 | cut -d'"' -f2)
echo "📦 Building BlueVault v${VERSION}"
echo ""

# Clean previous build
echo "🧹 Cleaning previous build..."
cargo clean

# Build release binary
echo "🔨 Building release binary..."
cargo build --release

# Check if build succeeded
if [ ! -f "target/release/bdarchive" ]; then
    echo "❌ Build failed!"
    exit 1
fi

echo ""
echo "✅ Build successful!"
echo ""
echo "📁 Binary location: $(pwd)/target/release/bdarchive"
echo "📏 Binary size: $(du -h target/release/bdarchive | cut -f1)"
echo ""
echo "🚀 To install system-wide (Linux/macOS):"
echo "   sudo cp target/release/bdarchive /usr/local/bin/bdarchive"
echo "   sudo chmod +x /usr/local/bin/bdarchive"
echo ""
echo "🧪 To test the binary:"
echo "   ./target/release/bdarchive --help"
echo ""

# Create distribution package if requested
if [ "$1" = "--package" ]; then
    echo "📦 Creating distribution package..."

    # Create package directory
    PKG_DIR="bluevault-${VERSION}"
    mkdir -p "${PKG_DIR}"

    # Copy binary and documentation
    cp target/release/bdarchive "${PKG_DIR}/"
    cp README.md "${PKG_DIR}/"
    cp LICENSE "${PKG_DIR}/"

    # Create tarball
    tar czf "${PKG_DIR}.tar.gz" "${PKG_DIR}"
    rm -rf "${PKG_DIR}"

    echo "📦 Package created: ${PKG_DIR}.tar.gz"
    echo "📏 Package size: $(du -h "${PKG_DIR}.tar.gz" | cut -f1)"
fi
