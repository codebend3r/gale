#!/bin/bash
# Benchmark gale vs stylelint on Bootstrap CSS (20x duplicated)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."
FIXTURES_DIR="$SCRIPT_DIR/fixtures"
BENCHMARK_FILE="$FIXTURES_DIR/bootstrap-20x.css"
BOOTSTRAP_FILE="$FIXTURES_DIR/bootstrap.css"

# Generate fixtures if needed
bash "$SCRIPT_DIR/generate-benchmark.sh"

# Create a minimal .stylelintrc.json in fixtures so Stylelint has config
if [ ! -f "$FIXTURES_DIR/.stylelintrc.json" ]; then
    echo '{ "extends": "stylelint-config-recommended" }' > "$FIXTURES_DIR/.stylelintrc.json"
fi

# Set up a local Stylelint installation in a temp directory
STYLELINT_DIR="$SCRIPT_DIR/.stylelint-local"
STYLELINT_BIN="$STYLELINT_DIR/node_modules/.bin/stylelint"

if [ ! -x "$STYLELINT_BIN" ]; then
    echo "Installing local Stylelint..."
    mkdir -p "$STYLELINT_DIR"
    (cd "$STYLELINT_DIR" && bun init -y 2>/dev/null && bun add stylelint stylelint-config-recommended)
fi

# Expose the local node_modules to the fixtures dir so both Gale and Stylelint
# resolve "extends": "stylelint-config-recommended" the same way
if [ ! -e "$FIXTURES_DIR/node_modules" ]; then
    ln -s "$STYLELINT_DIR/node_modules" "$FIXTURES_DIR/node_modules"
fi

echo ""
echo "============================================"
echo "  GALE vs STYLELINT BENCHMARK"
echo "============================================"
echo ""

GALE_BIN="$PROJECT_DIR/target/release/gale"

if [ ! -f "$GALE_BIN" ]; then
    echo "Building gale in release mode..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

# Check if hyperfine is available
if ! command -v hyperfine &>/dev/null; then
    echo "hyperfine not found. Install it: brew install hyperfine"
    echo ""
    echo "Falling back to manual timing..."
    echo ""

    echo "--- Gale (bootstrap.css) ---"
    time "$GALE_BIN" "$BOOTSTRAP_FILE" --quiet 2>/dev/null || true
    echo ""

    echo "--- Gale (bootstrap-20x.css) ---"
    time "$GALE_BIN" "$BENCHMARK_FILE" --quiet 2>/dev/null || true
    echo ""

    if [ -x "$STYLELINT_BIN" ]; then
        echo "--- Stylelint (bootstrap.css) ---"
        time "$STYLELINT_BIN" "$BOOTSTRAP_FILE" --quiet || true
        echo ""

        echo "--- Stylelint (bootstrap-20x.css) ---"
        time "$STYLELINT_BIN" "$BENCHMARK_FILE" --quiet || true
    else
        echo "stylelint not found. Local installation failed."
    fi
    exit 0
fi

# Use hyperfine for proper benchmarking
echo "--- bootstrap.css ($(wc -l < "$BOOTSTRAP_FILE" | tr -d ' ') lines) ---"
echo ""

HYPERFINE_ARGS=(--warmup 3 --min-runs 10 -i)
HYPERFINE_ARGS+=(-n "Gale" "$GALE_BIN $BOOTSTRAP_FILE --quiet")

if [ -x "$STYLELINT_BIN" ]; then
    HYPERFINE_ARGS+=(-n "Stylelint" "$STYLELINT_BIN $BOOTSTRAP_FILE --quiet")
fi

hyperfine "${HYPERFINE_ARGS[@]}" 2>&1 || true

echo ""
echo "--- bootstrap-20x.css ($(wc -l < "$BENCHMARK_FILE" | tr -d ' ') lines) ---"
echo ""

HYPERFINE_ARGS_20X=(--warmup 3 --min-runs 10 -i)
HYPERFINE_ARGS_20X+=(-n "Gale" "$GALE_BIN $BENCHMARK_FILE --quiet")

if [ -x "$STYLELINT_BIN" ]; then
    HYPERFINE_ARGS_20X+=(-n "Stylelint" "$STYLELINT_BIN $BENCHMARK_FILE --quiet")
fi

hyperfine "${HYPERFINE_ARGS_20X[@]}" 2>&1 || true
