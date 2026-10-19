# Benchmarks

Reproducible performance comparison of **Gale** vs **Stylelint** on real-world CSS/SCSS repositories.

## Quick start

```bash
./benchmarks/benchmark.sh
```

That's it. The script handles everything: building Gale, cloning test repos, installing dependencies, running benchmarks, and generating a results table.

## Prerequisites

| Tool | Install |
|------|---------|
| **Rust/cargo** | [rustup.rs](https://rustup.rs) |
| **Node.js >= 18** | [nodejs.org](https://nodejs.org) |
| **hyperfine** | `brew install hyperfine` (macOS) or `cargo install hyperfine` |
| **git** | Pre-installed on most systems |
| **python3** | Pre-installed on macOS/most Linux |

The script checks for all prerequisites and tells you what's missing.

## Usage

```bash
# Run all benchmarks (every repo in tests/differential/repos.json)
./benchmarks/benchmark.sh

# Run a specific repository
./benchmarks/benchmark.sh bootstrap
./benchmarks/benchmark.sh gutenberg

# Skip rebuilding Gale (use existing binary)
./benchmarks/benchmark.sh --skip-build

# Skip the parity/correctness test
./benchmarks/benchmark.sh --skip-parity

# Clean cloned repos and start fresh
./benchmarks/benchmark.sh --clean

# Use repos cloned elsewhere, and keep both linters' caches out of them
./benchmarks/benchmark.sh --repos-dir ~/corpus --cache-dir /tmp/gale-bench-cache
```

`--cache-dir` matters for repos whose Stylelint config turns `cache` on
(spectrum-css): each linter gets its own cache file there, deleted before
every run, so every timed run is cold and nothing is written into the clone.
Without it those repos are timed against a warm cache kept in the clone.

## What it measures

### Performance

Uses [hyperfine](https://github.com/sharkdp/hyperfine) with:
- **3 warmup runs** to fill OS/disk caches
- **5+ measured runs** for statistical reliability
- Both tools run on the **same files** with the **repo's own Stylelint config**

A linter run that exits with anything other than 0 (clean) or 2 (problems
found) has failed, for instance with 78 when its config cannot be loaded or
101 when Gale panics. Such a repo is shown as `FAIL (exit N)` and is not
timed, since a run that stops early is not a lint time. The results header
records the load average at the start and end of the run.

### Correctness (Parity)

After the speed benchmark, the script runs both linters with JSON output and compares diagnostics:

- **False Positives**: Gale reports something Stylelint does not
- **False Negatives**: Stylelint reports something Gale misses

This is the differential harness's comparison (`tests/harness.py`): every rule, plugin rules included, is compared by line, column, rule, severity and message, with no filters on either side.

## Test repositories

The benchmark uses the differential harness's corpus,
[`tests/differential/repos.json`](../tests/differential/repos.json): every
entry not marked `skip`, linted with its `globs` from its `cwd`. Run
`./benchmarks/benchmark.sh --help` for the list.

## Output

Results are saved to `benchmarks/results.md` and printed to stdout. Example:

```
## Performance

| Repository | Files | Stylelint | Gale    | Speedup |
|------------|------:|----------:|--------:|--------:|
| bootstrap  |    99 |    2.360s |  0.480s |    4.9x |
| gutenberg  |   312 |    5.120s |  0.890s |    5.8x |
```

## How it works

1. Builds Gale in release mode (`cargo build --release`, through `tests/harness.py build`)
2. Shallow-clones each test repository (idempotent -- skips if already present) with `tests/harness.py prepare`
3. Installs dependencies with `bun` (pnpm / Yarn Berry repos use their own tool so the pinned Stylelint version is honoured; `npm` only if bun cannot read the lockfile)
4. Runs `hyperfine` comparing `stylelint` (from repo's `node_modules`) vs `gale` (release binary)
5. Runs both linters with `--formatter json` and diffs the output for correctness
6. Generates a markdown results table

## Other benchmarks

The `benchmark-quick.sh` script in this directory runs a simpler single-file benchmark using a downloaded Bootstrap CSS file (and a 20x-duplicated variant). Use `benchmark.sh` for the full reproducible comparison.

## FAQ

**Q: Why not use globally installed Stylelint?**
Each repo has its own Stylelint config and plugins. Using the repo's `node_modules/.bin/stylelint` ensures the config resolves correctly.

**Q: Can I add more repositories?**
Add an entry to `tests/differential/repos.json`; the differential harness and this benchmark both pick it up. See [tests/differential/README.md](../tests/differential/README.md#repo-corpus) for the fields.

**Q: The parity test shows false negatives -- is that a bug?**
Maybe. False negatives on SCSS-specific rules are often caused by parser differences. File an issue with the specific rule and file if you find one.
