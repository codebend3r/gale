# Compatibility Matrix

Last updated: 2026-10-19

Gale is tested weekly against popular open-source repositories that use Stylelint.
Both tools run on the same files with the same config. Results are compared automatically.

| Repository | Stars | Files | Pass | FP | FN | Speedup |
|------------|-------|-------|------|----|----|---------|
| [twbs/bootstrap](https://github.com/twbs/bootstrap) | 168K | 102/117 | 87% | 77 | 0 | 60.2x |
| [grafana/grafana](https://github.com/grafana/grafana) | 62K | — | **run failed** | — | — | — |
| [wordpress/gutenberg](https://github.com/wordpress/gutenberg) | 10K | 611/613 | 100% | 9 | 0 | 23.4x |
| [primer/css](https://github.com/primer/css) | 12K | 111/113 | 98% | 3 | 3 | 13.7x |

> **1 run(s) did not complete** and are shown as `run failed`: grafana/grafana. A failed run is not evidence of parity — check the workflow logs.

### Legend

- **Files**: Matching files / Total files analyzed
- **Pass**: Percentage of files where Gale and Stylelint produce identical output
- **FP**: False positives — warnings Gale reports but Stylelint does not
- **FN**: False negatives — warnings Stylelint reports but Gale misses
- **Speedup**: How many times faster Gale is compared to Stylelint
- **run failed**: The differential run produced no comparable output; the row carries no parity information

### How to reproduce

```bash
git clone https://github.com/codebend3r/gale && cd gale
cargo build --release
python3 tests/differential/run.py bootstrap --benchmark
```

See [benchmarks/](benchmarks/) for the full benchmark script.
