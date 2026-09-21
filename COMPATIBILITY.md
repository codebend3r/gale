# Compatibility Matrix

Last updated: 2026-09-21

Gale is tested weekly against popular open-source repositories that use Stylelint.
Both tools run on the same files with the same config. Results are compared automatically.

| Repository | Stars | Files | Pass | FP | FN | Speedup |
|------------|-------|-------|------|----|----|---------|
| [twbs/bootstrap](https://github.com/twbs/bootstrap) | 168K | 93/98 | 95% | 5 | 0 | 22.5x |
| [grafana/grafana](https://github.com/grafana/grafana) | 62K | 10/10 | 100% | 0 | 0 | 18.3x |
| [wordpress/gutenberg](https://github.com/wordpress/gutenberg) | 10K | 586/597 | 98% | 18 | 20 | 20.3x |
| [primer/css](https://github.com/primer/css) | 12K | 113/113 | 100% | 0 | 0 | 5.5x |

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
