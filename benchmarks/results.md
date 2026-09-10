# Benchmark Results

> Generated on 2026-09-10 15:57 UTC
> System: Darwin arm64 | 25.6.0
> Node: v26.5.0 | Rust: 1.98.0

## Performance

| Repository | Files | Stylelint | Gale | Speedup |
|------------|------:|----------:|-----:|--------:|
| joomla | 169 | 1.914s | 0.045s | 42.5x |
| mattermost | 564 | 6.228s | 0.116s | 53.7x |

## Parity (Correctness)

| Repository | Files Tested | False Positives | False Negatives |
|------------|-------------:|----------------:|----------------:|
| joomla | 0 | 0 | 0 |
| mattermost | 1 | 1 | 0 |

---

*False Positives = Gale reports but Stylelint does not. False Negatives = Stylelint reports but Gale misses.*
*Only rules implemented in Gale are compared. Plugin-only rules are excluded.*

Reproduce these results: `./benchmarks/benchmark.sh`
