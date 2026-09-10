#!/usr/bin/env bash
#
# Build and package the Gale binary for npm distribution.
#
# Usage:
#   ./scripts/build-npm.sh                 # Build for current platform only
#   ./scripts/build-npm.sh --all           # Build for all supported platforms (requires cross)
#   ./scripts/build-npm.sh --version 0.2.0 # Set version in npm/package.json before building
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NPM_DIR="$ROOT/npm"

# --------------------------------------------------------------------------
# Parse arguments
# --------------------------------------------------------------------------
BUILD_ALL=false
VERSION=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --all)
      BUILD_ALL=true
      shift
      ;;
    --version)
      VERSION="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--all] [--version <semver>]"
      exit 1
      ;;
  esac
done

# --------------------------------------------------------------------------
# Version sync
# --------------------------------------------------------------------------
if [[ -n "$VERSION" ]]; then
  echo "==> Syncing version to $VERSION in npm/package.json..."
  cd "$NPM_DIR"
  npm version "$VERSION" --no-git-tag-version --allow-same-version 2>/dev/null
  echo "    npm package set to $VERSION"
fi

# --------------------------------------------------------------------------
# Supported Rust targets
# --------------------------------------------------------------------------
TARGETS=(
  "aarch64-apple-darwin"
  "x86_64-apple-darwin"
  "x86_64-unknown-linux-gnu"
  "aarch64-unknown-linux-gnu"
)

# --------------------------------------------------------------------------
# Detect current platform's Rust target
# --------------------------------------------------------------------------
detect_current_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os-$arch" in
    Darwin-arm64)   echo "aarch64-apple-darwin" ;;
    Darwin-x86_64)  echo "x86_64-apple-darwin" ;;
    Linux-x86_64)   echo "x86_64-unknown-linux-gnu" ;;
    Linux-aarch64)  echo "aarch64-unknown-linux-gnu" ;;
    *)
      echo "ERROR: Cannot detect Rust target for $os-$arch" >&2
      exit 1
      ;;
  esac
}

# --------------------------------------------------------------------------
# Build a single target
# --------------------------------------------------------------------------
build_target() {
  local rust_target="$1"
  local use_cross="$2"

  echo "==> Building for $rust_target..."

  if [[ "$use_cross" == "true" ]]; then
    cross build --release --target "$rust_target" --manifest-path "$ROOT/Cargo.toml"
    local src="$ROOT/target/$rust_target/release/gale"
  else
    cargo build --release --manifest-path "$ROOT/Cargo.toml"
    local src="$ROOT/target/release/gale"
  fi

  local dest="$NPM_DIR/bin/$rust_target/gale"
  echo "    Copying $src -> $dest"
  mkdir -p "$(dirname "$dest")"
  cp "$src" "$dest"
  chmod +x "$dest"
  echo "    Done: $rust_target"
}

# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------
if [[ "$BUILD_ALL" == "true" ]]; then
  echo "==> Building for ALL platforms (requires 'cross' — install with: cargo install cross)"
  echo ""

  # Check for cross
  if ! command -v cross &>/dev/null; then
    echo "ERROR: 'cross' is not installed."
    echo "Install it with: cargo install cross"
    echo ""
    echo "You also need Docker running for cross-compilation."
    exit 1
  fi

  CURRENT_TARGET="$(detect_current_target)"

  for rust_target in "${TARGETS[@]}"; do
    if [[ "$rust_target" == "$CURRENT_TARGET" ]]; then
      build_target "$rust_target" "false"
    else
      build_target "$rust_target" "true"
    fi
    echo ""
  done
else
  CURRENT_TARGET="$(detect_current_target)"
  build_target "$CURRENT_TARGET" "false"
fi

echo ""
echo "=========================================="
echo " Build complete!"
echo "=========================================="
echo ""
echo "Binaries ready in: $NPM_DIR/bin/"
