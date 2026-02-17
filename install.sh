#!/bin/bash

set -e

# Configuration
REPO="meet447/Enuma"
BINARY_NAME="Enuma"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Detect OS and architecture
detect_platform() {
    local OS=$(uname -s)
    local ARCH=$(uname -m)
    
    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64) echo "x86_64-unknown-linux-gnu" ;;
                aarch64) echo "aarch64-unknown-linux-gnu" ;;
                armv7l) echo "armv7-unknown-linux-gnueabihf" ;;
                *) echo "unsupported" ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64) echo "aarch64-apple-darwin" ;;
                *) echo "unsupported" ;;
            esac
            ;;
        *)
            echo "unsupported"
            ;;
    esac
}

# Get the latest release version
get_latest_version() {
    curl -s "https://api.github.com/repos/$REPO/releases/latest" | 
    grep '"tag_name":' | 
    sed -E 's/.*"([^"]+)".*/\1/'
}

# Main installation
main() {
    echo -e "${GREEN}Installing $BINARY_NAME...${NC}"
    
    # Detect platform
    PLATFORM=$(detect_platform)
    if [ "$PLATFORM" = "unsupported" ]; then
        echo -e "${RED}Error: Unsupported platform${NC}"
        echo "Supported platforms: Linux (x86_64, aarch64, armv7), macOS (x86_64, arm64)"
        exit 1
    fi
    
    echo "Detected platform: $PLATFORM"
    
    # Get latest version
    VERSION=$(get_latest_version)
    if [ -z "$VERSION" ]; then
        echo -e "${RED}Error: Could not determine latest version${NC}"
        exit 1
    fi
    
    echo "Latest version: $VERSION"
    
    # Create download URL
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/${BINARY_NAME}-${PLATFORM}.tar.gz"
    
    # Create temp directory
    TEMP_DIR=$(mktemp -d)
    trap "rm -rf $TEMP_DIR" EXIT
    
    # Download binary
    echo "Downloading from: $DOWNLOAD_URL"
    if ! curl -sL "$DOWNLOAD_URL" -o "$TEMP_DIR/${BINARY_NAME}.tar.gz"; then
        echo -e "${RED}Error: Failed to download binary${NC}"
        echo "Make sure you have a release at: https://github.com/$REPO/releases"
        exit 1
    fi
    
    # Extract binary
    cd "$TEMP_DIR"
    tar -xzf "${BINARY_NAME}.tar.gz"
    
    # Check if binary exists
    if [ ! -f "$BINARY_NAME" ]; then
        # Try without .tar.gz extension (just the binary)
        DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/${BINARY_NAME}-${PLATFORM}"
        if ! curl -sL "$DOWNLOAD_URL" -o "$BINARY_NAME"; then
            echo -e "${RED}Error: Could not find binary in archive${NC}"
            exit 1
        fi
        chmod +x "$BINARY_NAME"
    fi
    
    # Install binary
    echo -e "${YELLOW}Installing to $INSTALL_DIR/$BINARY_NAME${NC}"
    
    # Check if we need sudo
    if [ -w "$INSTALL_DIR" ]; then
        mv "$BINARY_NAME" "$INSTALL_DIR/"
    else
        echo "Need sudo access to install to $INSTALL_DIR"
        sudo mv "$BINARY_NAME" "$INSTALL_DIR/"
    fi
    
    # Verify installation
    if command -v "$BINARY_NAME" &> /dev/null; then
        echo -e "${GREEN}✓ $BINARY_NAME installed successfully!${NC}"
        echo ""
        echo "Run '$BINARY_NAME --help' to get started"
    else
        echo -e "${YELLOW}⚠ Installed but not in PATH${NC}"
        echo "Add $INSTALL_DIR to your PATH or run:"
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    fi
}

# Alternative: install to ~/.cargo/bin if it exists
if [ -d "$HOME/.cargo/bin" ]; then
    INSTALL_DIR="$HOME/.cargo/bin"
fi

# Check for INSTALL_DIR override
if [ -n "$1" ]; then
    INSTALL_DIR="$1"
fi

main
