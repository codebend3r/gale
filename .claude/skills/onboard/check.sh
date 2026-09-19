#!/usr/bin/env bash
#
# Read-only onboarding doctor for gale.
#
# Verifies every tool that `package.json` scripts reach for, and reports each
# one as ok / FAIL / warn. Changes nothing: the `onboard` skill reads this
# output and applies the matching fix.
#
# Exit status: 0 if every REQUIRED check passed, 1 otherwise.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"

RED=$'\033[31m'; GREEN=$'\033[32m'; YELLOW=$'\033[33m'; DIM=$'\033[2m'; OFF=$'\033[0m'
[[ -t 1 ]] || { RED=""; GREEN=""; YELLOW=""; DIM=""; OFF=""; }

FAILED=()
WARNED=()

ok()   { printf "  %sok%s    %-28s %s\n" "$GREEN" "$OFF" "$1" "${2:-}"; }
fail() { printf "  %sFAIL%s  %-28s %s\n" "$RED" "$OFF" "$1" "${2:-}"; FAILED+=("$1"); }
warn() { printf "  %swarn%s  %-28s %s\n" "$YELLOW" "$OFF" "$1" "${2:-}"; WARNED+=("$1"); }
head2() { printf "\n%s%s%s\n" "$DIM" "$1" "$OFF"; }

# Rustup keeps its shims in ~/.cargo/bin, which is only on PATH if the shell
# sources ~/.cargo/env. Look there directly so we can tell "not installed"
# apart from "installed but invisible to this shell".
have_cargo_shim() { [[ -x "$CARGO_BIN/$1" ]]; }
rustup_run() { PATH="$CARGO_BIN:$PATH" "$@"; }

head2 "JavaScript toolchain"

if command -v node &>/dev/null; then
  node_ver="$(node --version)"
  if node -e 'process.exit(parseInt(process.versions.node.split(".")[0],10) >= 20 ? 0 : 1)'; then
    ok "node" "$node_ver"
  else
    fail "node-version" "$node_ver, package.json engines wants >=20"
  fi
else
  fail "node" "not installed"
fi

command -v bun  &>/dev/null && ok "bun"  "$(bun --version)"        || fail "bun" "not installed"
command -v npm  &>/dev/null && ok "npm"  "v$(npm --version)"       || fail "npm" "not installed"
command -v git  &>/dev/null && ok "git"  "$(git --version | awk '{print $3}')" || fail "git" "not installed"

if [[ -x "$ROOT/node_modules/.bin/lefthook" ]]; then
  ok "node_modules" "lefthook present"
else
  fail "node_modules" "lefthook missing, run bun install"
fi

# Worktrees keep .git as a file and share hooks with the main checkout, so ask
# git where they actually live rather than assuming "$ROOT/.git/hooks".
HOOKS_DIR="$(cd "$ROOT" && git rev-parse --path-format=absolute --git-path hooks 2>/dev/null)"
if [[ -n "$HOOKS_DIR" && -x "$HOOKS_DIR/pre-commit" ]]; then
  ok "hooks" "lefthook installed"
else
  fail "hooks" "not installed, run bunx lefthook install"
fi

head2 "Rust toolchain"

if command -v cargo &>/dev/null; then
  ok "cargo" "$(cargo --version | awk '{print $2}')"
elif have_cargo_shim cargo; then
  fail "cargo-path" "installed at $CARGO_BIN but not on PATH"
else
  fail "cargo" "not installed"
fi

for component in rustfmt cargo-clippy; do
  label="${component#cargo-}"
  if rustup_run command -v "$component" &>/dev/null; then
    ok "$label" "$(rustup_run "$component" --version 2>/dev/null | awk '{print $2}')"
  else
    fail "component-$label" "rustup component missing"
  fi
done

if have_cargo_shim rustup; then
  installed_targets="$(rustup_run rustup target list --installed 2>/dev/null)"
  host_target="$(rustup_run rustc -vV 2>/dev/null | awk '/^host:/{print $2}')"
  # build-npm.sh --all builds both macOS arches natively; cross has no macOS image.
  wanted=("$host_target")
  [[ "$(uname -s)" == "Darwin" ]] && wanted+=("x86_64-apple-darwin" "aarch64-apple-darwin")
  for t in $(printf '%s\n' "${wanted[@]}" | sort -u); do
    [[ -n "$t" ]] || continue
    if grep -qx "$t" <<<"$installed_targets"; then
      ok "target" "$t"
    else
      fail "target-$t" "rustup target not installed"
    fi
  done
fi

head2 "Cross-compilation (build:all only)"

if rustup_run command -v cross &>/dev/null; then
  ok "cross" "$(rustup_run cross --version 2>/dev/null | head -1 | awk '{print $2}')"
else
  warn "cross" "not installed, build:all cannot make the Linux binaries"
fi

if command -v docker &>/dev/null; then
  if docker info &>/dev/null; then
    ok "docker" "daemon running"
  else
    warn "docker-daemon" "installed but not running, cross needs it"
  fi
else
  warn "docker" "not installed, cross needs it"
fi

head2 "Benchmarks (benchmark, benchmark:quick only)"

command -v hyperfine &>/dev/null && ok "hyperfine" "$(hyperfine --version | awk '{print $2}')" \
  || warn "hyperfine" "not installed, both benchmark scripts exit early"
command -v python3 &>/dev/null && ok "python3" "$(python3 --version | awk '{print $2}')" \
  || warn "python3" "not installed, benchmark.sh parses results with it"

head2 "Build artifacts"

if [[ -x "$ROOT/target/release/gale" ]]; then
  ok "release binary" "target/release/gale"
else
  warn "release-binary" "missing, test:api needs it (run build:release first)"
fi

printf "\n"
if (( ${#FAILED[@]} )); then
  printf "%s%d required check(s) failed:%s %s\n" "$RED" "${#FAILED[@]}" "$OFF" "${FAILED[*]}"
fi
if (( ${#WARNED[@]} )); then
  printf "%s%d optional check(s) warned:%s %s\n" "$YELLOW" "${#WARNED[@]}" "$OFF" "${WARNED[*]}"
fi
if (( ${#FAILED[@]} == 0 && ${#WARNED[@]} == 0 )); then
  printf "%sEverything needed to run every package.json script is installed.%s\n" "$GREEN" "$OFF"
fi

(( ${#FAILED[@]} == 0 ))
