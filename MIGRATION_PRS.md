# Migration PRs — Stylelint to Gale

Tracking PRs opened in the differential test corpus to migrate from Stylelint to Gale.

## Open PRs

| Repo | PR | Gale Version | Type | Status | Notes |
|------|------|-------------|------|--------|-------|
| twbs/bootstrap | [#42251](https://github.com/twbs/bootstrap/pull/42251) | 0.1.5 | Easy | Open | Simple package.json swap |
| grafana/grafana | [#121397](https://github.com/grafana/grafana/pull/121397) | 0.1.5 | Easy | Open | Uses yarn; brace glob |
| wordpress/gutenberg | [#76893](https://github.com/WordPress/gutenberg/pull/76893) | 0.1.5 | Medium | Open | Monorepo; wp-scripts wrapper |
| rsuite/rsuite | [#4564](https://github.com/rsuite/rsuite/pull/4564) | 0.1.5 | Easy | Open | dep + script swap |
| freeCodeCamp/freeCodeCamp | [#66675](https://github.com/freeCodeCamp/freeCodeCamp/pull/66675) | 0.1.5 | Easy | Open | dep + script swap, lint-staged |
| joomla/joomla-cms | [#47497](https://github.com/joomla/joomla-cms/pull/47497) | 0.1.5 | Easy | Open | Remove 4 plugin deps (all built-in) |
| mastodon/mastodon | [#38479](https://github.com/mastodon/mastodon/pull/38479) | 0.1.5 | Easy | Open | --custom-formatter works as-is |
| facebook/docusaurus | [#11859](https://github.com/facebook/docusaurus/pull/11859) | 0.1.5 | Medium | Open | Migrate stylelint-copyright → plugin/require-file-header-comment |
| SAP/fundamental-styles | [#6266](https://github.com/SAP/fundamental-styles/pull/6266) | 0.1.5 | Medium | Open | Replace nx-stylelint executor with nx:run-commands |
| alphagov/govuk-frontend | [#6893](https://github.com/alphagov/govuk-frontend/pull/6893) | 0.1.5 | Medium | Open | Update programmatic API imports in 6 test files |
| discourse/discourse | [#38949](https://github.com/discourse/discourse/pull/38949) | 0.1.5 | Medium | Open | Migrate no-breakpoint-mixin → scss/at-mixin-disallowed-list |

## Differential Testing Results

All repos tested with 0 false negatives and 0 false positives before opening PRs.

| Repo | Files | Warnings Matched | FN | FP |
|------|------:|-----------------:|---:|---:|
| bootstrap | 99 | — | 0 | 0 |
| grafana | — | — | 0 | 0 |
| gutenberg | 778 | — | 0 | 0 |
| rsuite | 207 | 784 | 0 | 0 |
| freeCodeCamp | 176 | 0 | 0 | 0 |
| joomla | 169 | 24 | 0 | 0 |
| mastodon | 61 | 0 | 0 | 0 |
| docusaurus | 98 | 0 | 0 | 0 |
| fundamental-styles | 392 | 0 | 0 | 0 |
| govuk-frontend | 149 | 0 | 0 | 0 |
| discourse | 356 | 0 | 0 | 0 |

## Not Yet Targeted

| Repo | Reason |
|------|--------|
| patternfly/patternfly | `liberty/use-logical-spec` JS plugin has no Gale equivalent |
| carbon-design-system/carbon | 3 third-party JS plugins without built-in equivalents |
| adobe/spectrum-css | 4 custom cross-file JS plugins + 2 third-party plugins |
| primer/css | 7 design-token plugins + KTLO mode (not accepting changes) |
| jupyterlab/jupyterlab | stylelint-prettier actively used (intentionally unsupported) |
| angular-components | Extglob `(src|docs)/**/*.+(css|scss)` not supported |
| wp-calypso | Gale hangs on 2238 files — needs investigation |
| mattermost | No stylelint in node_modules — needs setup check |
| slds | Stylelint v13 (message format mismatch, not a Gale bug) |
