#!/bin/bash
# Downloads Bootstrap CSS and duplicates it 20x (same strategy Stylelint uses internally)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"
mkdir -p "$FIXTURES_DIR"

BOOTSTRAP_URL="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.css"
BOOTSTRAP_FILE="$FIXTURES_DIR/bootstrap.css"
BENCHMARK_FILE="$FIXTURES_DIR/bootstrap-20x.css"

# Download Bootstrap CSS if not present
if [ ! -f "$BOOTSTRAP_FILE" ]; then
    echo "Downloading Bootstrap CSS..."
    curl -sL "$BOOTSTRAP_URL" -o "$BOOTSTRAP_FILE"
    echo "Downloaded $(wc -l < "$BOOTSTRAP_FILE") lines"
fi

# Create 20x duplicated version
if [ ! -f "$BENCHMARK_FILE" ]; then
    echo "Creating 20x duplicated benchmark file..."
    for i in $(seq 1 20); do
        cat "$BOOTSTRAP_FILE" >> "$BENCHMARK_FILE"
        echo "" >> "$BENCHMARK_FILE"
    done
    echo "Created $(wc -l < "$BENCHMARK_FILE") lines"
fi

echo ""
echo "Benchmark files ready:"
echo "  $BOOTSTRAP_FILE ($(wc -l < "$BOOTSTRAP_FILE") lines)"
echo "  $BENCHMARK_FILE ($(wc -l < "$BENCHMARK_FILE") lines)"
