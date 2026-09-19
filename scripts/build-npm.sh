#!/usr/bin/env bash
#
# Build and package the Gale binary for npm distribution.
#
# Usage:
#   ./scripts/build-npm.sh                 # Build for current platform only
#   ./scripts/build-npm.sh --all           # Build for all supported platforms (requires cross)
#   ./scripts/build-npm.sh --version 0.2.4 # Set version in npm/package.json before building
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NPM_DIR="$ROOT/npm"

# Rustup installs its shims (cargo, cross, ...) in ~/.cargo/bin, which is only
# on PATH if the user's shell sources ~/.cargo/env. Don't rely on that: the
# script is run from npm/bun scripts, CI, and editors that may inherit a
# minimal environment.
if [[ -d "${CARGO_HOME:-$HOME/.cargo}/bin" ]]; then
  case ":$PATH:" in
    *":${CARGO_HOME:-$HOME/.cargo}/bin:"*) ;;
    *) PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH" ;;
  esac
  export PATH
fi

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
# Which builder handles a target:
#   host   - the machine's own target, plain cargo, output in target/release
#   native - another target the host toolchain can link directly (the second
#            macOS arch), plain cargo with --target
#   cross  - needs a Docker image, so cross. cross has no macOS images, which
#            is why the second Darwin arch is built natively instead.
builder_for() {
  local rust_target="$1"
  local current_target="$2"

  if [[ "$rust_target" == "$current_target" ]]; then
    echo "host"
  elif [[ "$(uname -s)" == "Darwin" && "$rust_target" == *-apple-darwin ]]; then
    echo "native"
  else
    echo "cross"
  fi
}

build_target() {
  local rust_target="$1"
  local builder="$2"
  local src

  echo "==> Building for $rust_target ($builder)..."

  case "$builder" in
    host)
      cargo build --release --manifest-path "$ROOT/Cargo.toml"
      src="$ROOT/target/release/gale"
      ;;
    native)
      if ! rustup target list --installed | grep -qx "$rust_target"; then
        echo "    Installing missing rustup target $rust_target..."
        rustup target add "$rust_target"
      fi
      cargo build --release --target "$rust_target" --manifest-path "$ROOT/Cargo.toml"
      src="$ROOT/target/$rust_target/release/gale"
      ;;
    cross)
      cross build --release --target "$rust_target" --manifest-path "$ROOT/Cargo.toml"
      src="$ROOT/target/$rust_target/release/gale"
      ;;
    *)
      echo "ERROR: unknown builder '$builder' for $rust_target" >&2
      exit 1
      ;;
  esac

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
  echo "==> Building for ALL platforms"
  echo ""

  CURRENT_TARGET="$(detect_current_target)"

  # Only demand cross (and Docker) if a target actually needs it.
  NEEDS_CROSS=false
  for rust_target in "${TARGETS[@]}"; do
    if [[ "$(builder_for "$rust_target" "$CURRENT_TARGET")" == "cross" ]]; then
      NEEDS_CROSS=true
    fi
  done

  if [[ "$NEEDS_CROSS" == "true" ]]; then
    if ! command -v cross &>/dev/null; then
      echo "ERROR: 'cross' is not installed, and the Linux targets need it."
      echo "Install it with: cargo install cross"
      echo ""
      echo "You also need Docker running for cross-compilation."
      exit 1
    fi

    if ! docker info &>/dev/null; then
      echo "ERROR: Docker is not running, and 'cross' needs it to build the Linux targets."
      echo "Start Docker Desktop (or your daemon of choice) and re-run."
      exit 1
    fi
  fi

  for rust_target in "${TARGETS[@]}"; do
    build_target "$rust_target" "$(builder_for "$rust_target" "$CURRENT_TARGET")"
    echo ""
  done
else
  CURRENT_TARGET="$(detect_current_target)"
  build_target "$CURRENT_TARGET" "host"
fi

echo ""
echo "=========================================="
echo " Build complete!"
echo "=========================================="
echo ""
echo "Binaries ready in: $NPM_DIR/bin/"
