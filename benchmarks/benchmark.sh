#!/usr/bin/env bash
# =============================================================================
# Gale vs Stylelint -- Reproducible Benchmark
# =============================================================================
#
# Run this script to independently verify Gale's performance claims.
# It clones real-world CSS repositories, runs both linters via hyperfine,
# and produces a markdown results table.
#
# Usage:
#   ./benchmarks/benchmark.sh              # Full benchmark (all 21 repos)
#   ./benchmarks/benchmark.sh bootstrap    # Single repo
#   ./benchmarks/benchmark.sh --help       # Show help
#
# Requirements: cargo, node (>=18), hyperfine
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."
# Prefer the differential test clones to avoid re-downloading
DIFF_CLONES_DIR="$PROJECT_DIR/tests/differential/.clones"
if [ -d "$DIFF_CLONES_DIR" ]; then
  CLONES_DIR="$DIFF_CLONES_DIR"
else
  CLONES_DIR="$SCRIPT_DIR/.repos"
fi
RESULTS_FILE="$SCRIPT_DIR/results.md"
GALE_BIN="$PROJECT_DIR/target/release/gale"

WARMUP=3
MIN_RUNS=10

# The yarn/pnpm shims are corepack. When a repo pins a package manager version
# corepack has not cached yet, it asks "Do you want to continue? [Y/n]" on
# stdin. The install output is piped through tail, so that prompt is invisible
# and the run hangs forever. Never prompt; just download.
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0

# bun's global bin dir holds the corepack shims (yarn/pnpm) that Yarn Berry and
# pnpm repos need, and it is frequently not on PATH.
if command -v bun &>/dev/null; then
  BUN_GLOBAL_BIN="$(bun pm bin -g 2>/dev/null || true)"
  if [ -n "$BUN_GLOBAL_BIN" ] && [ -d "$BUN_GLOBAL_BIN" ]; then
    export PATH="$BUN_GLOBAL_BIN:$PATH"
  fi
fi

# Test repositories: name|repo|branch|glob_pattern|search_dir[|cwd]
# cwd (optional) is the directory both linters run from, for monorepos whose
# package.json and Stylelint config live below the clone root.
REPOS=(
  "bootstrap|twbs/bootstrap|main|scss/**/*.scss|scss"
  "carbon|carbon-design-system/carbon|main|packages/**/*.scss|packages"
  "freecodecamp|freeCodeCamp/freeCodeCamp|main|client/**/*.css|client"
  "grafana|grafana/grafana|main|public/**/*.{css,scss}|public"
  "govuk-frontend|alphagov/govuk-frontend|main|packages/**/*.scss|packages"
  "gutenberg|wordpress/gutenberg|trunk|packages/**/*.scss|packages"
  "material-ui|mui/material-ui|master|packages/**/*.css|packages"
  "patternfly|patternfly/patternfly|main|src/**/*.scss|src"
  "primer-css|primer/css|main|src/**/*.scss|src"
  "spectrum-css|adobe/spectrum-css|main|components/**/*.css|components"
  "angular-components|angular/components|main|src/**/*.scss|src"
  "docusaurus|facebook/docusaurus|main|packages/**/*.css|packages"
  "discourse|discourse/discourse|main|app/assets/stylesheets/**/*.scss|app/assets/stylesheets"
  "wp-calypso|Automattic/wp-calypso|trunk|client/**/*.scss|client"
  "mattermost|mattermost/mattermost|master|**/*.{css,scss}|webapp/channels|webapp/channels"
  "mastodon|mastodon/mastodon|main|app/javascript/styles/**/*.scss|app/javascript/styles"
  "jupyterlab|jupyterlab/jupyterlab|main|packages/**/*.css|packages"
  "joomla|joomla/joomla-cms|5.4-dev|build/media_source/**/*.{css,scss}|build"
  "slds|salesforce-ux/design-system|main|ui/**/*.scss|ui"
  "rsuite|rsuite/rsuite|main|src/**/*.scss|src"
  "fundamental-styles|SAP/fundamental-styles|main|packages/**/*.scss|packages"
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info()    { echo -e "${BLUE}==>${NC} ${BOLD}$*${NC}"; }
success() { echo -e "${GREEN}==>${NC} ${BOLD}$*${NC}"; }
warn()    { echo -e "${YELLOW}warning:${NC} $*"; }
error()   { echo -e "${RED}error:${NC} $*"; exit 1; }

# Pick the installer. bun is the project's package manager, so use it wherever
# it can honour the repo's own lockfile: bun.lock, package-lock.json (bun
# migrates it), and Yarn v1 lockfiles. pnpm and Yarn Berry lockfiles are not
# migratable, and bun would silently resolve fresh and drift from the pinned
# stylelint version, so those repos use their own tool.
detect_pm() {
  local dir="$1"
  if [ -f "$dir/pnpm-lock.yaml" ]; then
    echo "pnpm"
  elif [ -f "$dir/yarn.lock" ] && grep -q '^__metadata:' "$dir/yarn.lock"; then
    echo "yarn"
  else
    echo "bun"
  fi
}

# Walk up from $1 (stopping at $2) to the nearest directory containing $3.
find_up() {
  local dir="$1" stop="$2" target="$3"
  while :; do
    if [ -e "$dir/$target" ]; then
      echo "$dir"
      return 0
    fi
    [ "$dir" = "$stop" ] && return 1
    dir="$(dirname "$dir")"
  done
}

install_deps() {
  local dir="$1" clone_dir="$2"
  # Monorepos: install from the nearest package.json at or above the work dir.
  dir="$(find_up "$dir" "$clone_dir" package.json)" || {
    warn "No package.json found at or above $1"
    return 1
  }
  if [ -d "$dir/node_modules" ]; then
    echo "    node_modules already present, skipping install"
    return 0
  fi

  local pm
  pm=$(detect_pm "$dir")
  echo "    Installing dependencies with $pm..."

  # Yarn Berry defaults to PnP, which leaves no node_modules/.bin/stylelint.
  if [ "$pm" = "yarn" ] && [ -f "$dir/.yarnrc.yml" ] && ! grep -q nodeLinker "$dir/.yarnrc.yml"; then
    printf '\nnodeLinker: node-modules\n' >> "$dir/.yarnrc.yml"
  fi

  local status=0
  case "$pm" in
    bun)   (cd "$dir" && bun install --ignore-scripts 2>&1 | tail -1) || status=$? ;;
    pnpm)  (cd "$dir" && pnpm install --ignore-scripts --no-frozen-lockfile 2>&1 | tail -1) || status=$? ;;
    yarn)  (cd "$dir" && yarn install --mode skip-build 2>&1 | tail -1) || status=$? ;;
  esac

  # Some npm lockfiles fail bun's integrity check (Mattermost, whose lockfile
  # sits at the workspace root above the install dir). npm is the only tool
  # that can honour those, so fall back to it rather than skip the repo.
  if [ "$status" -ne 0 ] && [ "$pm" = "bun" ] && command -v npm &>/dev/null; then
    warn "bun install failed in $dir (exit $status); retrying with npm"
    rm -rf "$dir/node_modules"
    status=0
    (cd "$dir" && npm install --ignore-scripts 2>&1 | tail -1) || status=$?
    pm=npm
  fi

  if [ "$status" -ne 0 ]; then
    warn "$pm install failed in $dir (exit $status)"
    return 1
  fi
}

count_files() {
  local dir="$1"
  local search_dir="$2"
  # Use find to count matching files (portable)
  { find "$dir/$search_dir" -not -path "*/node_modules/*" -not -path "*/.git/*" \( -name "*.scss" -o -name "*.css" -o -name "*.less" \) 2>/dev/null || true; } | wc -l | tr -d ' '
}

# ---------------------------------------------------------------------------
# Prerequisites check
# ---------------------------------------------------------------------------

check_prereqs() {
  info "Checking prerequisites..."

  local missing=0

  if ! command -v cargo &>/dev/null; then
    warn "cargo not found. Install Rust: https://rustup.rs"
    missing=1
  fi

  if ! command -v node &>/dev/null; then
    warn "node not found. Install Node.js >= 18: https://nodejs.org"
    missing=1
  fi

  if ! command -v hyperfine &>/dev/null; then
    warn "hyperfine not found. Install it:"
    echo "    macOS:  brew install hyperfine"
    echo "    Linux:  cargo install hyperfine  (or apt/dnf)"
    echo "    Other:  https://github.com/sharkdp/hyperfine#installation"
    missing=1
  fi

  if ! command -v python3 &>/dev/null; then
    warn "python3 not found. Required for result parsing and parity tests."
    missing=1
  fi

  if ! command -v git &>/dev/null; then
    warn "git not found."
    missing=1
  fi

  if [ "$missing" -eq 1 ]; then
    error "Missing prerequisites. Install them and re-run."
  fi

  success "All prerequisites found"
}

# ---------------------------------------------------------------------------
# Build Gale
# ---------------------------------------------------------------------------

build_gale() {
  if [ -f "$GALE_BIN" ]; then
    info "Gale release binary already built (use 'cargo build --release' to rebuild)"
  else
    info "Building Gale in release mode..."
    (cd "$PROJECT_DIR" && cargo build --release)
    success "Build complete"
  fi

  echo "    Binary: $GALE_BIN"
  echo "    Version: $("$GALE_BIN" --version 2>/dev/null || echo 'unknown')"
}

# ---------------------------------------------------------------------------
# Clone repos
# ---------------------------------------------------------------------------

clone_repo() {
  local name="$1" repo="$2" branch="$3"
  local dest="$CLONES_DIR/$name"

  if [ -d "$dest" ]; then
    echo "    [skip] $name already cloned"
    return 0
  fi

  echo "    [clone] $repo @ $branch"
  if ! git clone --depth 1 --branch "$branch" "https://github.com/$repo.git" "$dest" 2>&1 | tail -1; then
    rm -rf "$dest"
    return 1
  fi
}

# ---------------------------------------------------------------------------
# Run benchmarks
# ---------------------------------------------------------------------------

run_benchmark_for_repo() {
  local name="$1" repo="$2" branch="$3" glob_pattern="$4" search_dir="$5" cwd="${6:-}"
  local clone_dir="$CLONES_DIR/$name"
  local work_dir="$clone_dir${cwd:+/$cwd}"
  local hyperfine_json="$SCRIPT_DIR/.hyperfine-${name}.json"

  info "Benchmarking: $name"

  # Clone
  if ! clone_repo "$name" "$repo" "$branch"; then
    warn "Could not clone $repo @ $branch. Skipping."
    echo "$name|0|SKIP|SKIP|SKIP" >> "$SCRIPT_DIR/.benchmark-results.txt"
    return 0
  fi

  # Install deps
  if ! install_deps "$work_dir" "$clone_dir"; then
    warn "Could not install $name's dependencies. Skipping."
    echo "$name|0|SKIP|SKIP|SKIP" >> "$SCRIPT_DIR/.benchmark-results.txt"
    return 0
  fi

  # Check stylelint is available (workspaces hoist it above the work dir)
  local stylelint_bin
  stylelint_bin="$(find_up "$work_dir" "$clone_dir" node_modules/.bin/stylelint)/node_modules/.bin/stylelint" || {
    warn "Stylelint not found in $name's node_modules. Skipping."
    echo "$name|0|SKIP|SKIP|SKIP" >> "$SCRIPT_DIR/.benchmark-results.txt"
    return 0
  }

  # Count files
  local file_count
  file_count=$(count_files "$clone_dir" "$search_dir")
  echo "    Files matching pattern: $file_count"

  # Pre-run validation: run both linters once and verify non-empty output
  info "Validating both linters produce output..."

  local stylelint_check gale_check
  stylelint_check=$(cd "$work_dir" && "$stylelint_bin" "$glob_pattern" --formatter json 2>&1 || true)
  gale_check=$(cd "$work_dir" && "$GALE_BIN" "$glob_pattern" --formatter json 2>&1 || true)

  if [ -z "$stylelint_check" ]; then
    warn "Stylelint produced no output for $name. Skipping this repo."
    echo "$name|$file_count|SKIP|SKIP|SKIP" >> "$SCRIPT_DIR/.benchmark-results.txt"
    return 0
  fi

  if [ -z "$gale_check" ]; then
    warn "Gale produced no output for $name. Skipping this repo."
    echo "$name|$file_count|SKIP|SKIP|SKIP" >> "$SCRIPT_DIR/.benchmark-results.txt"
    return 0
  fi

  success "Both linters produced output. Proceeding with benchmark."

  # Run hyperfine
  info "Running hyperfine ($MIN_RUNS runs, $WARMUP warmup)..."

  hyperfine \
    --warmup "$WARMUP" \
    --min-runs "$MIN_RUNS" \
    --export-json "$hyperfine_json" \
    --command-name "stylelint" \
    "cd $work_dir && $stylelint_bin '$glob_pattern' >/dev/null 2>>$SCRIPT_DIR/.benchmark-stderr.log || true" \
    --command-name "gale" \
    "cd $work_dir && $GALE_BIN '$glob_pattern' >/dev/null 2>>$SCRIPT_DIR/.benchmark-stderr.log || true"

  # Parse results from JSON
  local stylelint_mean gale_mean speedup
  stylelint_mean=$(python3 -c "
import json, sys
with open('$hyperfine_json') as f:
    data = json.load(f)
for r in data['results']:
    if r['command'] == 'stylelint':
        print(f\"{r['mean']:.3f}\")
" 2>/dev/null || echo "N/A")

  gale_mean=$(python3 -c "
import json, sys
with open('$hyperfine_json') as f:
    data = json.load(f)
for r in data['results']:
    if r['command'] == 'gale':
        print(f\"{r['mean']:.3f}\")
" 2>/dev/null || echo "N/A")

  if [ "$stylelint_mean" != "N/A" ] && [ "$gale_mean" != "N/A" ]; then
    speedup=$(python3 -c "print(f'{$stylelint_mean / $gale_mean:.1f}')" 2>/dev/null || echo "?")
  else
    speedup="?"
  fi

  echo "$name|$file_count|${stylelint_mean}s|${gale_mean}s|${speedup}x" >> "$SCRIPT_DIR/.benchmark-results.txt"

  success "$name: Gale is ${speedup}x faster (${gale_mean}s vs ${stylelint_mean}s)"
}

# ---------------------------------------------------------------------------
# Run parity (differential) test
# ---------------------------------------------------------------------------

run_parity_test() {
  local name="$1" repo="$2" branch="$3" glob_pattern="$4" search_dir="$5" cwd="${6:-}"
  local clone_dir="$CLONES_DIR/$name"
  local work_dir="$clone_dir${cwd:+/$cwd}"

  local stylelint_bin
  stylelint_bin="$(find_up "$work_dir" "$clone_dir" node_modules/.bin/stylelint)/node_modules/.bin/stylelint" || {
    echo "$name|SKIP|SKIP|SKIP" >> "$SCRIPT_DIR/.parity-results.txt"
    return 0
  }

  info "Parity test: $name"

  # Run both linters with JSON output, save to temp files to avoid
  # shell quoting issues with embedded JSON
  # Stylelint 16+ prints its report to stderr, so keep both streams.
  local stylelint_tmp="$SCRIPT_DIR/.parity-stylelint-${name}.json"
  local stylelint_err="$SCRIPT_DIR/.parity-stylelint-${name}.stderr"
  local gale_tmp="$SCRIPT_DIR/.parity-gale-${name}.json"
  local gale_err="$SCRIPT_DIR/.parity-gale-${name}.stderr"

  (cd "$work_dir" && "$stylelint_bin" "$glob_pattern" --formatter json 2>"$stylelint_err" || true) > "$stylelint_tmp"
  (cd "$work_dir" && "$GALE_BIN" "$glob_pattern" --formatter json 2>"$gale_err" || true) > "$gale_tmp"

  # Compare using Python for robust JSON diffing
  local parity_result
  parity_result=$(python3 - "$stylelint_tmp" "$stylelint_err" "$gale_tmp" "$gale_err" "$work_dir" <<'PYEOF'
import json, os, sys

# Stylelint reports absolute source paths and Gale reports paths relative to
# the working directory; compare both relative to it or nothing ever matches.
WORK_DIR = sys.argv[5]

def read_report(stdout_path, stderr_path):
    """Return the JSON report text, whichever stream it was written to."""
    for path in (stdout_path, stderr_path):
        try:
            with open(path) as f:
                text = f.read()
        except FileNotFoundError:
            continue
        start = text.find("[")
        if start != -1 and text[start:].lstrip().startswith(("[{", "[]")):
            return text[start:].strip()
    return ""

def parse_warnings(stdout_path, stderr_path):
    """Extract (file, line, column, rule) tuples from linter JSON output."""
    text = read_report(stdout_path, stderr_path)
    if not text:
        return set()
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        return set()
    warnings = set()
    for entry in data:
        source = entry.get("source", "")
        if os.path.isabs(source):
            source = os.path.relpath(source, WORK_DIR)
        for w in entry.get("warnings", []):
            rule = w.get("rule", "")
            line = w.get("line", 0)
            col = w.get("column", 0)
            warnings.add((source, line, col, rule))
    return warnings

stylelint_w = parse_warnings(sys.argv[1], sys.argv[2])
gale_w = parse_warnings(sys.argv[3], sys.argv[4])

# Only compare rules that Gale implements (all 161 from ALL_RULE_NAMES)
gale_rules = {
    "alpha-value-notation", "annotation-no-unknown",
    "at-rule-allowed-list", "at-rule-descriptor-no-unknown",
    "at-rule-descriptor-value-no-unknown", "at-rule-disallowed-list",
    "at-rule-empty-line-before", "at-rule-no-deprecated",
    "at-rule-no-unknown", "at-rule-no-vendor-prefix",
    "at-rule-prelude-no-invalid", "at-rule-property-required-list",
    "block-no-empty", "block-no-redundant-nested-style-rules",
    "color-function-alias-notation", "color-function-notation",
    "color-hex-alpha", "color-hex-case", "color-hex-length",
    "color-named", "color-no-hex", "color-no-invalid-hex",
    "comment-empty-line-before", "comment-no-empty", "comment-pattern",
    "comment-whitespace-inside", "comment-word-disallowed-list",
    "container-name-pattern", "custom-media-pattern",
    "custom-property-empty-line-before",
    "custom-property-no-missing-var-function", "custom-property-pattern",
    "declaration-block-no-duplicate-custom-properties",
    "declaration-block-no-duplicate-properties",
    "declaration-block-no-redundant-longhand-properties",
    "declaration-block-no-shorthand-property-overrides",
    "declaration-block-single-line-max-declarations",
    "declaration-empty-line-before", "declaration-no-important",
    "declaration-property-unit-allowed-list",
    "declaration-property-unit-disallowed-list",
    "declaration-property-value-allowed-list",
    "declaration-property-value-disallowed-list",
    "declaration-property-value-keyword-no-deprecated",
    "declaration-property-value-no-unknown",
    "display-notation", "font-family-name-quotes",
    "font-family-no-duplicate-names",
    "font-family-no-missing-generic-family-keyword",
    "font-weight-notation", "function-allowed-list",
    "function-calc-no-unspaced-operator", "function-disallowed-list",
    "function-linear-gradient-no-nonstandard-direction",
    "function-name-case", "function-no-unknown",
    "function-url-no-scheme-relative", "function-url-quotes",
    "function-url-scheme-allowed-list", "function-url-scheme-disallowed-list",
    "hue-degree-notation", "import-notation",
    "keyframe-block-no-duplicate-selectors",
    "keyframe-declaration-no-important", "keyframe-selector-notation",
    "keyframes-name-pattern", "layer-name-pattern",
    "length-zero-no-unit", "lightness-notation",
    "max-line-length", "max-nesting-depth",
    "media-feature-name-allowed-list", "media-feature-name-disallowed-list",
    "media-feature-name-no-unknown", "media-feature-name-no-vendor-prefix",
    "media-feature-name-unit-allowed-list",
    "media-feature-name-value-allowed-list",
    "media-feature-name-value-no-unknown", "media-feature-range-notation",
    "media-query-no-invalid", "media-type-no-deprecated",
    "named-grid-areas-no-invalid",
    "nesting-selector-no-missing-scoping-root",
    "no-descending-specificity", "no-duplicate-at-import-rules",
    "no-duplicate-selectors", "no-empty-source",
    "no-invalid-double-slash-comments",
    "no-invalid-position-at-import-rule",
    "no-invalid-position-declaration", "no-irregular-whitespace",
    "no-unknown-animations", "number-leading-zero", "number-max-precision",
    "order/properties-alphabetical-order", "order/properties-order",
    "property-allowed-list", "property-disallowed-list",
    "property-no-deprecated", "property-no-unknown",
    "property-no-vendor-prefix", "rule-empty-line-before",
    "rule-nesting-at-rule-required-list",
    "rule-selector-property-disallowed-list",
    "scss/at-extend-no-missing-placeholder", "scss/at-if-no-null",
    "scss/at-rule-no-unknown", "scss/comment-no-empty",
    "scss/declaration-nested-properties-no-divided-groups",
    "scss/dollar-variable-no-missing-interpolation",
    "scss/function-quote-no-quoted-strings-inside",
    "scss/function-unquote-no-unquoted-strings-inside",
    "scss/load-no-partial-leading-underscore", "scss/load-partial-extension",
    "scss/no-duplicate-mixins", "scss/no-global-function-names",
    "scss/operator-no-newline-after", "scss/operator-no-newline-before",
    "scss/operator-no-unspaced", "selector-anb-no-unmatchable",
    "selector-attribute-name-disallowed-list",
    "selector-attribute-operator-allowed-list",
    "selector-attribute-operator-disallowed-list",
    "selector-attribute-quotes", "selector-class-pattern",
    "selector-combinator-allowed-list", "selector-combinator-disallowed-list",
    "selector-disallowed-list", "selector-id-pattern",
    "selector-max-attribute", "selector-max-class",
    "selector-max-combinators", "selector-max-compound-selectors",
    "selector-max-id", "selector-max-pseudo-class",
    "selector-max-specificity", "selector-max-type",
    "selector-max-universal", "selector-nested-pattern",
    "selector-no-qualifying-type", "selector-no-vendor-prefix",
    "selector-not-notation", "selector-pseudo-class-allowed-list",
    "selector-pseudo-class-disallowed-list",
    "selector-pseudo-class-no-unknown",
    "selector-pseudo-element-allowed-list",
    "selector-pseudo-element-colon-notation",
    "selector-pseudo-element-disallowed-list",
    "selector-pseudo-element-no-unknown", "selector-type-case",
    "selector-type-no-unknown",
    "shorthand-property-no-redundant-values", "string-no-newline",
    "string-quotes", "syntax-string-no-invalid", "time-min-milliseconds",
    "unit-allowed-list", "unit-disallowed-list", "unit-no-unknown",
    "value-keyword-case", "value-no-vendor-prefix",
}

stylelint_filtered = {w for w in stylelint_w if w[3] in gale_rules}
gale_filtered = {w for w in gale_w if w[3] in gale_rules}

false_negatives = stylelint_filtered - gale_filtered
false_positives = gale_filtered - stylelint_filtered

total_files = len({w[0] for w in stylelint_filtered | gale_filtered})

print(f"{total_files}|{len(false_positives)}|{len(false_negatives)}")
PYEOF
  ) || parity_result="ERR|ERR|ERR"

  rm -f "$stylelint_tmp" "$gale_tmp"

  local total_files fp fn
  total_files=$(echo "$parity_result" | cut -d'|' -f1)
  fp=$(echo "$parity_result" | cut -d'|' -f2)
  fn=$(echo "$parity_result" | cut -d'|' -f3)

  echo "$name|$total_files|$fp|$fn" >> "$SCRIPT_DIR/.parity-results.txt"
  echo "    Files tested: $total_files | False positives: $fp | False negatives: $fn"
}

# ---------------------------------------------------------------------------
# Generate results markdown
# ---------------------------------------------------------------------------

generate_results() {
  info "Generating results..."

  local date_str
  date_str=$(date -u +"%Y-%m-%d %H:%M UTC")

  cat > "$RESULTS_FILE" <<EOF
# Benchmark Results

> Generated on $date_str
> System: $(uname -s) $(uname -m) | $(uname -r)
> Node: $(node --version 2>/dev/null || echo 'N/A') | Rust: $(rustc --version 2>/dev/null | cut -d' ' -f2 || echo 'N/A')

## Performance

| Repository | Files | Stylelint | Gale | Speedup |
|------------|------:|----------:|-----:|--------:|
EOF

  while IFS='|' read -r name files stylelint_time gale_time speedup; do
    echo "| $name | $files | $stylelint_time | $gale_time | $speedup |" >> "$RESULTS_FILE"
  done < "$SCRIPT_DIR/.benchmark-results.txt"

  cat >> "$RESULTS_FILE" <<'EOF'

## Parity (Correctness)

| Repository | Files Tested | False Positives | False Negatives |
|------------|-------------:|----------------:|----------------:|
EOF

  while IFS='|' read -r name total_files fp fn; do
    echo "| $name | $total_files | $fp | $fn |" >> "$RESULTS_FILE"
  done < "$SCRIPT_DIR/.parity-results.txt"

  cat >> "$RESULTS_FILE" <<'EOF'

---

*False Positives = Gale reports but Stylelint does not. False Negatives = Stylelint reports but Gale misses.*
*Only rules implemented in Gale are compared. Plugin-only rules are excluded.*

Reproduce these results: `./benchmarks/benchmark.sh`
EOF

  success "Results written to $RESULTS_FILE"
  echo ""
  cat "$RESULTS_FILE"
}

# ---------------------------------------------------------------------------
# Cleanup temp files
# ---------------------------------------------------------------------------

cleanup_temp() {
  rm -f "$SCRIPT_DIR/.benchmark-results.txt"
  rm -f "$SCRIPT_DIR/.parity-results.txt"
  rm -f "$SCRIPT_DIR/.benchmark-stderr.log"
  rm -f "$SCRIPT_DIR"/.hyperfine-*.json
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

usage() {
  echo "Usage: $0 [OPTIONS] [REPO...]"
  echo ""
  echo "Run Gale vs Stylelint benchmarks on real-world repositories."
  echo ""
  echo "Repos:    bootstrap, carbon, freecodecamp, grafana, govuk-frontend,"
  echo "          gutenberg, material-ui, patternfly, primer-css,"
  echo "          spectrum-css, angular-components, docusaurus,"
  echo "          discourse, wp-calypso, mattermost (default: all)"
  echo ""
  echo "Options:"
  echo "  --help          Show this help"
  echo "  --skip-build    Skip building Gale (use existing binary)"
  echo "  --skip-parity   Skip the parity/correctness test"
  echo "  --clean         Remove cloned repos and start fresh"
  echo ""
  echo "Prerequisites: cargo, node (>=18), hyperfine, git, python3"
}

main() {
  local skip_build=0
  local skip_parity=0
  local clean=0
  local selected_repos=()

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --help|-h)    usage; exit 0 ;;
      --skip-build) skip_build=1 ;;
      --skip-parity) skip_parity=1 ;;
      --clean)      clean=1 ;;
      -*)           error "Unknown option: $1" ;;
      *)            selected_repos+=("$1") ;;
    esac
    shift
  done

  echo ""
  echo "============================================"
  echo "  GALE vs STYLELINT -- REPRODUCIBLE BENCHMARK"
  echo "============================================"
  echo ""

  # Prerequisites
  check_prereqs

  # Clean if requested
  if [ "$clean" -eq 1 ]; then
    info "Cleaning cloned repos..."
    rm -rf "$CLONES_DIR"
  fi

  mkdir -p "$CLONES_DIR"

  # Build
  if [ "$skip_build" -eq 0 ]; then
    build_gale
  else
    if [ ! -f "$GALE_BIN" ]; then
      error "Gale binary not found at $GALE_BIN. Run without --skip-build first."
    fi
    info "Using existing Gale binary"
  fi

  # Filter repos if specific ones requested
  local repos_to_run=()
  if [ ${#selected_repos[@]} -gt 0 ]; then
    for sel in "${selected_repos[@]}"; do
      for entry in "${REPOS[@]}"; do
        local entry_name
        entry_name=$(echo "$entry" | cut -d'|' -f1)
        if [ "$entry_name" = "$sel" ]; then
          repos_to_run+=("$entry")
        fi
      done
    done
    if [ ${#repos_to_run[@]} -eq 0 ]; then
      error "No matching repos found. Available: bootstrap, carbon, freecodecamp, grafana, govuk-frontend, gutenberg, material-ui, patternfly, primer-css"
    fi
  else
    repos_to_run=("${REPOS[@]}")
  fi

  # Clean temp files
  cleanup_temp

  echo ""

  # Run benchmarks
  for entry in "${repos_to_run[@]}"; do
    IFS='|' read -r name repo branch glob_pattern search_dir cwd <<< "$entry"
    run_benchmark_for_repo "$name" "$repo" "$branch" "$glob_pattern" "$search_dir" "$cwd"
    echo ""
  done

  # Run parity tests
  if [ "$skip_parity" -eq 0 ]; then
    for entry in "${repos_to_run[@]}"; do
      IFS='|' read -r name repo branch glob_pattern search_dir cwd <<< "$entry"
      run_parity_test "$name" "$repo" "$branch" "$glob_pattern" "$search_dir" "$cwd"
      echo ""
    done
  fi

  # Generate results
  generate_results

  # Cleanup
  cleanup_temp

  echo ""
  success "Done! Results saved to benchmarks/results.md"
}

main "$@"
