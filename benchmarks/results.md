# Benchmark Results

> Generated on 2026-09-11 03:00 UTC
> System: Darwin arm64 | 25.6.0
> Node: v26.5.0 | Rust: 1.98.0

## Performance

| Repository | Files | Stylelint | Gale | Speedup |
|------------|------:|----------:|-----:|--------:|
| bootstrap | 99 | 2.798s | 0.088s | 31.8x |
| carbon | 1263 | 4.389s | 0.195s | 22.5x |
| freecodecamp | 91 | 0.978s | 0.026s | 37.6x |
| grafana | 10 | 0.722s | 0.051s | 14.2x |
| govuk-frontend | 326 | 4.816s | 0.029s | 166.1x |
| gutenberg | 729 | 2.835s | 0.172s | 16.5x |
| material-ui | 39 | 0.646s | 0.053s | 12.2x |
| patternfly | 213 | 6.653s | 0.313s | 21.3x |
| primer-css | 113 | 2.215s | 0.236s | 9.4x |
| spectrum-css | 236 | 4.304s | 0.260s | 16.6x |
| angular-components | 627 | 3.028s | 0.112s | 27.0x |
| docusaurus | 116 | 0.213s | 0.032s | 6.7x |
| discourse | 377 | 2.899s | 0.191s | 15.2x |
| wp-calypso | 2051 | 23.167s | 0.676s | 34.3x |
| mattermost | 564 | 6.085s | 0.115s | 52.9x |
| mastodon | 36 | 3.216s | 0.268s | 12.0x |
| jupyterlab | 211 | 2.920s | 0.178s | 16.4x |
| joomla | 169 | 1.556s | 0.034s | 45.8x |
| slds | 446 | 2.641s | 0.229s | 11.5x |
| rsuite | 207 | 3.801s | 0.160s | 23.8x |
| fundamental-styles | 392 | 6.800s | 0.121s | 56.2x |

## Parity (Correctness)

| Repository | Files Tested | False Positives | False Negatives |
|------------|-------------:|----------------:|----------------:|
| bootstrap | 1 | 1 | 0 |
| carbon | 0 | 0 | 0 |
| freecodecamp | 0 | 0 | 0 |
| grafana | 0 | 0 | 0 |
| govuk-frontend | 0 | 0 | 0 |
| gutenberg | 12 | 15 | 16 |
| material-ui | 0 | 0 | 0 |
| patternfly | 24 | 117 | 0 |
| primer-css | 0 | 0 | 0 |
| spectrum-css | 1 | 1 | 0 |
| angular-components | 0 | 0 | 0 |
| docusaurus | 0 | 0 | 0 |
| discourse | 0 | 0 | 0 |
| wp-calypso | 0 | 0 | 0 |
| mattermost | 0 | 0 | 0 |
| mastodon | 0 | 0 | 0 |
| jupyterlab | 88 | 788 | 4 |
| joomla | 0 | 0 | 0 |
| slds | 26 | 0 | 0 |
| rsuite | 99 | 638 | 0 |
| fundamental-styles | 1 | 1 | 0 |

---

*False Positives = Gale reports but Stylelint does not. False Negatives = Stylelint reports but Gale misses.*
*Only rules implemented in Gale are compared. Plugin-only rules are excluded.*

Reproduce these results: `./benchmarks/benchmark.sh`
