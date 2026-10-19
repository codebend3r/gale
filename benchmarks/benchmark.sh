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
#   ./benchmarks/benchmark.sh              # Full benchmark (every repo in tests/differential/repos.json)
#   ./benchmarks/benchmark.sh bootstrap    # Single repo
#   ./benchmarks/benchmark.sh --help       # Show help
#
# Requirements: cargo, node (>=18), hyperfine, python3
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# Lists, clones, installs and compares the repos of the shared corpus,
# tests/differential/repos.json, the same way the differential harness does.
HARNESS="$PROJECT_DIR/tests/harness.py"
# Prefer the differential test clones to avoid re-downloading
DIFF_CLONES_DIR="$PROJECT_DIR/tests/differential/.clones"
if [ -d "$DIFF_CLONES_DIR" ]; then
  CLONES_DIR="$DIFF_CLONES_DIR"
else
  CLONES_DIR="$SCRIPT_DIR/.repos"
fi
# Set by --cache-dir: where both linters keep their caches (see cache_file).
CACHE_DIR=""

# The 1, 5 and 15 minute load averages, as `uptime` prints them.
load_average() {
  uptime | sed -E 's/.*load averages?: //'
}
LOAD_AT_START="$(load_average)"
RESULTS_FILE="$SCRIPT_DIR/results.md"
GALE_BIN="$PROJECT_DIR/target/release/gale"

WARMUP=3
MIN_RUNS=10

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

# Count the CSS/SCSS/Less files under the repo's search paths.
count_files() {
  local clone_dir="$1" search_dirs=() path
  shift
  for path in "$@"; do
    search_dirs+=("$clone_dir/$path")
  done
  { find "${search_dirs[@]}" -not -path "*/node_modules/*" -not -path "*/.git/*" \( -name "*.scss" -o -name "*.css" -o -name "*.less" \) 2>/dev/null || true; } | wc -l | tr -d ' '
}

# Both linters exit 0 when clean and 2 when they report problems. Anything
# else means the run failed (78: a config that would not load, 101: a Gale
# panic, 1: a fatal error), so it produced no lint result and its time means
# nothing.
lint_ok() {
  [ "$1" -eq 0 ] || [ "$1" -eq 2 ]
}

# Appended to every timed command so that hyperfine stops on a failed run
# instead of timing it: exit 0 for the two lint statuses, 1 for anything else.
LINT_STATUS_CHECK='case $? in 0|2) ;; *) exit 1 ;; esac'

# The cache file one linter uses on one repo under --cache-dir, or nothing.
#
# Some repos turn the cache on in their config (spectrum-css). Pointed at a
# file that is deleted before every run, the linters run cold, as on any
# other repo, and never write a cache into the clone.
cache_file() {
  if [ -n "$CACHE_DIR" ]; then
    echo "$CACHE_DIR/$1-$2.cache"
  fi
}

# Time one linter command with hyperfine, writing its JSON export. Fails when
# any run of the command fails. With a cache file, it is deleted before every
# run and the command is pointed at it.
time_linter() {
  local label="$1" command="$2" json="$3" cache="${4:-}"
  local prepare=()
  if [ -n "$cache" ]; then
    command="$command --cache-location '$cache'"
    prepare=(--prepare "rm -f '$cache'")
  fi
  hyperfine \
    --warmup "$WARMUP" \
    --min-runs "$MIN_RUNS" \
    ${prepare[@]+"${prepare[@]}"} \
    --export-json "$json" \
    --command-name "$label" \
    "$command >/dev/null 2>>$SCRIPT_DIR/.benchmark-stderr.log; $LINT_STATUS_CHECK"
}

# Run one linter once with JSON output, starting from an empty cache under
# --cache-dir. Usage: run_once <cache-file|""> <stdout> <stderr> <dir> <cmd...>
run_once() {
  local cache="$1" out="$2" err="$3" dir="$4"
  shift 4
  local cache_args=()
  if [ -n "$cache" ]; then
    rm -f "$cache"
    cache_args=(--cache-location "$cache")
  fi
  (cd "$dir" && "$@" --formatter json ${cache_args[@]+"${cache_args[@]}"}) >"$out" 2>"$err"
}

# Mean time in seconds from a hyperfine JSON export, or N/A.
mean_time() {
  python3 -c "
import json, sys
with open(sys.argv[1]) as f:
    print(f\"{json.load(f)['results'][0]['mean']:.3f}\")
" "$1" 2>/dev/null || echo "N/A"
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
    warn "python3 not found. Required for the repo list, installs and parity tests."
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
  python3 "$HARNESS" build || error "Gale build failed"
  echo "    Binary: $GALE_BIN"
  echo "    Version: $("$GALE_BIN" --version 2>/dev/null || echo 'unknown')"
}

# ---------------------------------------------------------------------------
# Run benchmarks
# ---------------------------------------------------------------------------

# Usage: run_benchmark_for_repo <name> <stylelint-bin|""> <cwd> <globs> <paths>
# An empty Stylelint binary means the repo could not be prepared.
run_benchmark_for_repo() {
  local name="$1" stylelint_bin="$2" cwd="$3" globs paths
  read -r -a globs <<< "$4"
  read -r -a paths <<< "$5"
  local clone_dir="$CLONES_DIR/$name"
  local work_dir="$clone_dir${cwd:+/$cwd}"

  info "Benchmarking: $name"

  if [ -z "$stylelint_bin" ]; then
    warn "Could not clone $name, install its dependencies, or find its Stylelint. Skipping."
    echo "$name|0|SKIP|SKIP|SKIP" >> "$SCRIPT_DIR/.benchmark-results.txt"
    return 0
  fi

  # Count files
  local file_count
  file_count=$(count_files "$clone_dir" "${paths[@]}")
  echo "    Files matching pattern: $file_count"

  # Pre-run validation: run both linters once. A linter that fails is
  # reported as failed rather than timed: a crash or a config that will not
  # load finishes quickly and would otherwise be published as a speedup.
  info "Validating both linters run successfully..."

  local check_out="$SCRIPT_DIR/.benchmark-check-${name}.out"
  local check_err="$SCRIPT_DIR/.benchmark-check-${name}.err"
  local stylelint_cache gale_cache
  stylelint_cache=$(cache_file "$name" stylelint)
  gale_cache=$(cache_file "$name" gale)
  local stylelint_status=0 gale_status=0
  run_once "$stylelint_cache" "$check_out" "$check_err" "$work_dir" \
    "$stylelint_bin" "${globs[@]}" || stylelint_status=$?
  if ! lint_ok "$stylelint_status"; then
    warn "Stylelint failed on $name (exit $stylelint_status). Not timing this repo:"
    tail -5 "$check_err" | sed 's/^/    /'
    echo "$name|$file_count|FAIL (exit $stylelint_status)|-|-" >> "$SCRIPT_DIR/.benchmark-results.txt"
    rm -f "$check_out" "$check_err"
    return 0
  fi

  run_once "$gale_cache" "$check_out" "$check_err" "$work_dir" \
    "$GALE_BIN" "${globs[@]}" || gale_status=$?
  if ! lint_ok "$gale_status"; then
    warn "Gale failed on $name (exit $gale_status). Not timing this repo:"
    tail -5 "$check_err" | sed 's/^/    /'
    echo "$name|$file_count|-|FAIL (exit $gale_status)|-" >> "$SCRIPT_DIR/.benchmark-results.txt"
    rm -f "$check_out" "$check_err"
    return 0
  fi
  rm -f "$check_out" "$check_err"

  success "Both linters ran successfully. Proceeding with benchmark."

  # Run hyperfine, one linter at a time so a failure is attributed.
  info "Running hyperfine ($MIN_RUNS runs, $WARMUP warmup)..."

  local quoted_globs
  quoted_globs=$(printf " '%s'" "${globs[@]}")
  local stylelint_json="$SCRIPT_DIR/.hyperfine-${name}-stylelint.json"
  local gale_json="$SCRIPT_DIR/.hyperfine-${name}-gale.json"
  if ! time_linter stylelint "cd $work_dir && $stylelint_bin$quoted_globs" "$stylelint_json" "$stylelint_cache"; then
    warn "A timed Stylelint run failed on $name; see $SCRIPT_DIR/.benchmark-stderr.log"
    echo "$name|$file_count|FAIL (timed run)|-|-" >> "$SCRIPT_DIR/.benchmark-results.txt"
    return 0
  fi
  if ! time_linter gale "cd $work_dir && $GALE_BIN$quoted_globs" "$gale_json" "$gale_cache"; then
    warn "A timed Gale run failed on $name; see $SCRIPT_DIR/.benchmark-stderr.log"
    echo "$name|$file_count|-|FAIL (timed run)|-" >> "$SCRIPT_DIR/.benchmark-results.txt"
    return 0
  fi

  # Parse results from JSON
  local stylelint_mean gale_mean speedup
  stylelint_mean=$(mean_time "$stylelint_json")
  gale_mean=$(mean_time "$gale_json")

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

# Usage: run_parity_test <name> <stylelint-bin|""> <cwd> <globs>
run_parity_test() {
  local name="$1" stylelint_bin="$2" cwd="$3" globs
  read -r -a globs <<< "$4"
  local clone_dir="$CLONES_DIR/$name"
  local work_dir="$clone_dir${cwd:+/$cwd}"

  if [ -z "$stylelint_bin" ]; then
    echo "$name|SKIP|SKIP|SKIP" >> "$SCRIPT_DIR/.parity-results.txt"
    return 0
  fi

  info "Parity test: $name"

  # Stylelint 16+ prints its report to stderr, so keep both streams.
  local stylelint_tmp="$SCRIPT_DIR/.parity-stylelint-${name}.json"
  local stylelint_err="$SCRIPT_DIR/.parity-stylelint-${name}.stderr"
  local gale_tmp="$SCRIPT_DIR/.parity-gale-${name}.json"
  local gale_err="$SCRIPT_DIR/.parity-gale-${name}.stderr"

  local stylelint_status=0 gale_status=0
  run_once "$(cache_file "$name" stylelint)" "$stylelint_tmp" "$stylelint_err" "$work_dir" \
    "$stylelint_bin" "${globs[@]}" || stylelint_status=$?
  run_once "$(cache_file "$name" gale)" "$gale_tmp" "$gale_err" "$work_dir" \
    "$GALE_BIN" "${globs[@]}" || gale_status=$?

  # A failed run has no result to compare; an empty one would count every
  # warning of the other linter as a mismatch.
  local failed=""
  lint_ok "$stylelint_status" || failed="Stylelint exit $stylelint_status"
  lint_ok "$gale_status" || failed="${failed:+$failed, }Gale exit $gale_status"
  if [ -n "$failed" ]; then
    rm -f "$stylelint_tmp" "$stylelint_err" "$gale_tmp" "$gale_err"
    echo "$name|FAIL ($failed)|-|-" >> "$SCRIPT_DIR/.parity-results.txt"
    echo "    Not compared: $failed"
    return 0
  fi

  # The differential harness's comparison: every rule, no filters.
  local parity_result
  parity_result=$(python3 "$HARNESS" compare "$name" "$work_dir" \
    "$stylelint_tmp" "$stylelint_err" "$gale_tmp" "$gale_err") \
    || parity_result="FAIL (no JSON report)|-|-"

  rm -f "$stylelint_tmp" "$stylelint_err" "$gale_tmp" "$gale_err"

  local total_files fp fn
  IFS='|' read -r total_files fp fn <<< "$parity_result"

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
> Load average: $LOAD_AT_START at the start, $(load_average) at the end
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
*FAIL = the linter exited with something other than 0 (clean) or 2 (problems found), e.g. 78 for a config it could not load or 101 for a crash. Failed runs are neither timed nor compared.*
*Every rule is compared, plugin rules included, by line, column, rule, severity and message, as in `tests/differential/run.py`.*

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
  echo "Repos (default: all), from tests/differential/repos.json:"
  python3 "$HARNESS" repos 2>/dev/null | cut -d'|' -f1 | paste -sd ' ' - | fold -s -w 70 | sed 's/^/  /'
  echo ""
  echo "Options:"
  echo "  --help          Show this help"
  echo "  --skip-build    Skip building Gale (use existing binary)"
  echo "  --skip-parity   Skip the parity/correctness test"
  echo "  --clean         Remove cloned repos and start fresh"
  echo "  --repos-dir DIR Use (or clone into) DIR instead of the default clone directory"
  echo "  --cache-dir DIR Keep both linters' caches in DIR, emptied before every run,"
  echo "                  so runs are cold and nothing is written into the clones"
  echo ""
  echo "Prerequisites: cargo, node (>=18), hyperfine, git, python3"
}

main() {
  local skip_build=0
  local skip_parity=0
  local clean=0
  local repos_dir=0
  local selected_repos=()

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --help|-h)    usage; exit 0 ;;
      --skip-build) skip_build=1 ;;
      --skip-parity) skip_parity=1 ;;
      --clean)      clean=1 ;;
      --repos-dir)  CLONES_DIR="$(cd "${2:?--repos-dir needs a directory}" && pwd)"; repos_dir=1; shift ;;
      --cache-dir)  mkdir -p "${2:?--cache-dir needs a directory}"; CACHE_DIR="$(cd "$2" && pwd)"; shift ;;
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

  # Clean if requested. Never delete a directory the caller pointed at.
  if [ "$clean" -eq 1 ] && [ "$repos_dir" -eq 1 ]; then
    error "--clean would delete the --repos-dir directory; refusing"
  fi
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

  # The requested repos, or all of them: name|cwd|globs|paths per line.
  local listing entry
  listing=$(python3 "$HARNESS" repos ${selected_repos[@]+"${selected_repos[@]}"}) \
    || error "No matching repos found."
  local repos_to_run=()
  while IFS= read -r entry; do
    repos_to_run+=("$entry")
  done <<< "$listing"

  # Clean temp files
  cleanup_temp

  echo ""

  # Run benchmarks, remembering each repo's Stylelint for the parity tests
  # (empty when the repo could not be cloned or installed).
  local i name cwd globs paths stylelint_bins=()
  for i in "${!repos_to_run[@]}"; do
    IFS='|' read -r name cwd globs paths <<< "${repos_to_run[$i]}"
    stylelint_bins[i]=$(python3 "$HARNESS" prepare "$name" --clones-dir "$CLONES_DIR") \
      || stylelint_bins[i]=""
    run_benchmark_for_repo "$name" "${stylelint_bins[$i]}" "$cwd" "$globs" "$paths"
    echo ""
  done

  # Run parity tests
  if [ "$skip_parity" -eq 0 ]; then
    for i in "${!repos_to_run[@]}"; do
      IFS='|' read -r name cwd globs paths <<< "${repos_to_run[$i]}"
      run_parity_test "$name" "${stylelint_bins[$i]}" "$cwd" "$globs"
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
