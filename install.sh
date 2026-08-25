#!/usr/bin/env bash
# Install Devlish - builds the Rust compiler and adds it to your PATH
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "Devlish Installer"
echo ""

# Check for Rust toolchain
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is required but not installed."
  echo "Install it from https://rustup.rs:"
  echo ""
  echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  echo ""
  exit 1
fi

# Build the compiler in release mode
echo "Building devlish-core..."
cargo build --manifest-path "$SCRIPT_DIR/crates/devlish_core/Cargo.toml" --release 2>&1 | tail -1
echo ""

BINARY="$SCRIPT_DIR/crates/devlish_core/target/release/devlish-core"
if [ ! -x "$BINARY" ]; then
  echo "Build failed: binary not found at $BINARY"
  exit 1
fi

# Verify it works
VERSION=$("$BINARY" version 2>/dev/null)
echo "Built: $VERSION"
echo ""

# Make bin/devlish executable
chmod +x "$SCRIPT_DIR/bin/devlish"

# Add to PATH
BIN_DIR="$SCRIPT_DIR/bin"
SHELL_CONFIG=""
if [ -n "$ZSH_VERSION" ] || [ -f "$HOME/.zshrc" ]; then
  SHELL_CONFIG="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
  SHELL_CONFIG="$HOME/.bashrc"
elif [ -f "$HOME/.profile" ]; then
  SHELL_CONFIG="$HOME/.profile"
fi

if [ -n "$SHELL_CONFIG" ]; then
  if grep -q "$BIN_DIR" "$SHELL_CONFIG" 2>/dev/null; then
    echo "Already in PATH ($SHELL_CONFIG)"
  else
    echo "" >> "$SHELL_CONFIG"
    echo "# Devlish" >> "$SHELL_CONFIG"
    echo "export PATH=\"$BIN_DIR:\$PATH\"" >> "$SHELL_CONFIG"
    echo "Added to PATH in $SHELL_CONFIG"
    echo "Run: source $SHELL_CONFIG"
  fi
else
  echo "Add this to your shell config:"
  echo "  export PATH=\"$BIN_DIR:\$PATH\""
fi

# macOS: register .dvl file type for Quick Look and Finder previews
if [ "$(uname)" = "Darwin" ]; then
  APP_BUNDLE="$SCRIPT_DIR/macos/Devlish.app"
  INSTALL_DIR="$HOME/Applications"

  if [ -d "$APP_BUNDLE" ]; then
    mkdir -p "$INSTALL_DIR"
    # Remove old version if present, then copy fresh
    rm -rf "$INSTALL_DIR/Devlish.app"
    cp -R "$APP_BUNDLE" "$INSTALL_DIR/Devlish.app"

    # Register the UTI with Launch Services
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
      -f "$INSTALL_DIR/Devlish.app" 2>/dev/null

    echo "Registered .dvl file type (Quick Look, Spotlight, Finder)"
  fi
fi

echo ""
echo "Done. Try:"
echo "  devlish help"
echo "  devlish new my_project"
