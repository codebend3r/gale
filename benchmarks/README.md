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
# Run all benchmarks (Bootstrap + Gutenberg)
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
```

## What it measures

### Performance

Uses [hyperfine](https://github.com/sharkdp/hyperfine) with:
- **3 warmup runs** to fill OS/disk caches
- **5+ measured runs** for statistical reliability
- Both tools run on the **same files** with the **repo's own Stylelint config**

### Correctness (Parity)

After the speed benchmark, the script runs both linters with JSON output and compares diagnostics:

- **False Positives**: Gale reports something Stylelint does not
- **False Negatives**: Stylelint reports something Gale misses

Only rules that Gale implements are compared -- plugin-only rules (e.g., `scss/*`, `@stylistic/*`) are excluded.

## Test repositories

| Repository | Description | Config |
|------------|-------------|--------|
| [Bootstrap](https://github.com/twbs/bootstrap) | Industry-standard CSS framework, mature SCSS config | `stylelint-config-standard` + scss |
| [Gutenberg](https://github.com/wordpress/gutenberg) | WordPress editor, mix of legacy and modern CSS | `stylelint-config-recommended` + scss |

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

1. Builds Gale in release mode (`cargo build --release`)
2. Shallow-clones each test repository (idempotent -- skips if already present)
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
Edit the `REPOS` array at the top of `benchmark.sh`. Format: `name|github_org/repo|branch|glob_pattern|search_dir`.

**Q: The parity test shows false negatives -- is that a bug?**
Maybe. False negatives on SCSS-specific rules are often caused by parser differences. File an issue with the specific rule and file if you find one.
