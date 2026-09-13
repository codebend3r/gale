# Differential Testing: Gale vs Stylelint

This harness validates that Gale is a true **drop-in replacement** for Stylelint by running both tools on real-world public repositories and comparing their JSON output.

## How it works

1. **Clone** — Shallow-clones a curated list of public repos that use Stylelint
2. **Install** — Installs npm dependencies (auto-detects npm/yarn/pnpm/bun)
3. **Lint** — Runs both Stylelint and Gale on the same files using the repo's own config
4. **Compare** — Normalizes JSON output and compares warnings by `(line, column, rule, severity, text)`
5. **Report** — Generates a parity score, rule-level breakdown, and per-file diffs

## Quick start

```bash
# Run all repos (builds Gale first)
python tests/differential/run.py

# Run a specific repo
python tests/differential/run.py bootstrap

# Skip the build step (use existing binary)
python tests/differential/run.py bootstrap --skip-build

# Only test .css files (skip SCSS/Less)
python tests/differential/run.py bootstrap --css-only

# Benchmark: measure and compare execution times
python tests/differential/run.py bootstrap --benchmark

# Force re-clone repos
python tests/differential/run.py bootstrap --update

# List available repos
python tests/differential/run.py --list
```

## Repo corpus

The repos are defined in `repos.json`. Each entry specifies:

| Field | Description |
|-------|-------------|
| `name` | Short identifier used in CLI and output files |
| `repo` | GitHub `owner/repo` |
| `branch` | Branch to clone |
| `paths` | Directories to search for CSS files |
| `notes` | Human context about the repo's CSS setup |

Current repos:

| Name | Repo | Why |
|------|------|-----|
| grafana | grafana/grafana | Simple config, mostly disabled rules |
| bootstrap | twbs/bootstrap | Industry standard, mature SCSS config |
| gutenberg | wordpress/gutenberg | Mix of legacy and modern CSS |
| material-ui | mui/material-ui | Pure CSS, config in external package |
| primer-css | primer/css | GitHub's design system, custom plugins |
| carbon | carbon-design-system/carbon | IBM design system, very strict config |

## Output

### Terminal report

```
======================================================================
  REPORT: bootstrap
======================================================================
  Files analyzed:        99
  Files matching:        4
  Files with diffs:      95
  Matching warnings:     0
  Stylelint-only (FN):   0       ← Gale missed these (false negatives)
  Gale-only (FP):        3169    ← Gale reported these incorrectly (false positives)
  Parity score:          0.0%

  Rule breakdown:
  Rule                                               FN       FP
  ──────────────────────────────────────────────────────────────────
  at-rule-no-unknown                                 0        558
  comment-empty-line-before                          0        1110
  ...
```

### Saved files

Raw results are saved in `results/` (git-ignored):

- `{name}_stylelint.json` — Raw Stylelint JSON output
- `{name}_gale.json` — Raw Gale JSON output
- `{name}_report.json` — Comparison report with all diffs

## What the metrics mean

| Metric | Meaning |
|--------|---------|
| **Parity score** | `matching / (matching + FN + FP) * 100`. 100% = perfect drop-in |
| **FN (False Negatives)** | Warnings Stylelint reports but Gale misses. Gale needs to detect these |
| **FP (False Positives)** | Warnings Gale reports but Stylelint doesn't. Gale is being too aggressive |
| **Rule breakdown** | Per-rule FN/FP counts to identify which rules need work |

## Benchmarking

Use the `--benchmark` flag to measure execution time for both Stylelint and Gale:

```bash
python tests/differential/run.py bootstrap --benchmark
```

When enabled, each per-repo report includes a **Performance** section:

```
Performance:
  Stylelint:   4.32s
  Gale:        0.18s
  Speedup:     24.0x faster
```

The final summary table also gains a **Speedup** column showing the relative speed advantage for each repo.

Timing covers the full linting execution (all file batches) but excludes cloning, dependency installation, and result comparison.

## Filtering

- **Stylelint plugin rules** (e.g. `scss/*`, `@stylistic/*`) are automatically filtered out of Stylelint's output since Gale only implements core rules
- Use `--css-only` to skip SCSS/Less files and test only pure CSS parsing

## Known gaps

These are the main categories of discrepancies found so far:

### 1. Config resolution (`extends` with npm packages)

Gale cannot resolve `extends` values that reference npm packages (e.g. `"stylelint-config-standard"`). It only supports `gale:recommended` and `gale:all` presets. When it encounters an unknown extends, it skips it and falls back to running all rules.

**Impact:** This is the #1 source of false positives. Most repos extend shared configs.

### 2. SCSS/Less syntax handling

When processing SCSS files, Gale treats SCSS-specific syntax (variables `$var`, mixins `@include`, conditionals `@if`, interpolation `#{}`) as errors because the CSS parser doesn't understand them.

**Impact:** Rules like `at-rule-no-unknown`, `comment-no-empty`, `custom-property-pattern` produce many false positives on SCSS files.

### 3. Priority for fixing

Based on bootstrap test results, fixing these would have the highest impact:

1. **Config: npm extends resolution** — Would eliminate most FPs by honoring the repo's config
2. **SCSS parser integration** — `at-rule-no-unknown` (558 FP), `comment-*` rules on `//` comments
3. **Rule-specific tuning** — `no-descending-specificity` (374 FP) may need SCSS nesting awareness

## Adding a new repo

1. Add an entry to `repos.json`
2. Run `python tests/differential/run.py <name>` to test it
3. Check that Stylelint runs successfully (needs `stylelint` in devDependencies)

## Architecture

```
tests/differential/
  run.py          — Main harness script
  repos.json      — Curated list of test repos
  .gitignore      — Ignores .clones/ and results/
  README.md       — This file
  .clones/        — (git-ignored) Cloned repositories
  results/        — (git-ignored) Raw JSON outputs and reports
```
