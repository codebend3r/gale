# Benchmark Results

> Generated on 2026-10-01 05:34 UTC
> System: Darwin arm64 | 27.0.0
> Load average: 12.07 6.15 4.63 at the start, 21.90 31.76 30.73 at the end
> Node: v26.5.0 | Rust: 1.98.0

## Performance

| Repository | Files | Stylelint | Gale | Speedup |
|------------|------:|----------:|-----:|--------:|
| bootstrap | 99 | 2.308s | 0.040s | 57.7x |
| carbon | 1263 | 4.678s | 0.135s | 34.7x |
| freecodecamp | 91 | 0.835s | 0.015s | 55.7x |
| grafana | 10 | 0.619s | 0.055s | 11.3x |
| govuk-frontend | 326 | - | FAIL (exit 78) | - |
| gutenberg | 729 | 5.090s | 0.219s | 23.2x |
| material-ui | 39 | 0.709s | 0.051s | 13.9x |
| patternfly | 213 | 6.245s | 0.063s | 99.1x |
| primer-css | 113 | 1.809s | 0.155s | 11.7x |
| spectrum-css | 236 | 5.458s | 0.053s | 103.0x |
| angular-components | 627 | 3.508s | 0.047s | 74.6x |
| docusaurus | 116 | FAIL (exit 78) | - | - |
| discourse | 377 | 3.685s | 0.039s | 94.5x |
| wp-calypso | 2051 | 23.903s | 0.459s | 52.1x |
| mattermost | 564 | 7.063s | 0.073s | 96.8x |
| mastodon | 36 | 3.195s | 0.066s | 48.4x |
| jupyterlab | 211 | 2.760s | 0.042s | 65.7x |
| joomla | 169 | 1.739s | 0.023s | 75.6x |
| slds | 446 | 2.336s | 0.049s | 47.7x |
| rsuite | 207 | 3.470s | 0.037s | 93.8x |
| fundamental-styles | 392 | 6.080s | 0.050s | 121.6x |

## Parity (Correctness)

| Repository | Files Tested | False Positives | False Negatives |
|------------|-------------:|----------------:|----------------:|
| bootstrap | 1 | 1 | 0 |
| carbon | 10 | 17 | 0 |
| freecodecamp | 0 | 0 | 0 |
| grafana | 0 | 0 | 0 |
| govuk-frontend | FAIL (Gale exit 78) | - | - |
| gutenberg | 13 | 17 | 0 |
| material-ui | 0 | 0 | 0 |
| patternfly | 24 | 117 | 0 |
| primer-css | 0 | 0 | 0 |
| spectrum-css | 1 | 0 | 0 |
| angular-components | 0 | 0 | 0 |
| docusaurus | FAIL (Stylelint exit 78, Gale exit 78) | - | - |
| discourse | 0 | 0 | 0 |
| wp-calypso | 0 | 0 | 0 |
| mattermost | 0 | 0 | 0 |
| mastodon | 0 | 0 | 0 |
| jupyterlab | 88 | 788 | 4 |
| joomla | 0 | 0 | 0 |
| slds | 26 | 0 | 0 |
| rsuite | 99 | 0 | 0 |
| fundamental-styles | 1 | 1 | 0 |

---

*False Positives = Gale reports but Stylelint does not. False Negatives = Stylelint reports but Gale misses.*
*FAIL = the linter exited with something other than 0 (clean) or 2 (problems found), e.g. 78 for a config it could not load or 101 for a crash. Failed runs are neither timed nor compared.*
*Only rules implemented in Gale are compared. Plugin-only rules are excluded.*

Reproduce these results: `./benchmarks/benchmark.sh`
