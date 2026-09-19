# Gale

**An extremely fast CSS linter. Drop-in replacement for Stylelint.**

[![npm version](https://img.shields.io/npm/v/@codebend3r/gale)](https://www.npmjs.com/package/@codebend3r/gale)
[![CI](https://github.com/codebend3r/gale/actions/workflows/sanity-check.yml/badge.svg)](https://github.com/codebend3r/gale/actions)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Gale reads your existing `.stylelintrc`, runs the same rules, and produces the same output — typically **10-50x faster** on real projects.

One line change in your `package.json`. No config migration.

> **Compatibility note:** Gale targets **Stylelint v17** semantics. If your project uses Stylelint v16 or earlier, you may see minor differences in edge cases (e.g., how `selector-max-*` rules count selectors inside `:is()`, `:has()`, `:where()`). These match the behavior you would get after upgrading to Stylelint v17.

## Benchmarks

Real-world benchmarks using [hyperfine](https://github.com/sharkdp/hyperfine) (10 runs, 3 warmup) on an Apple M1 Max, generated on 2026-09-11 by `./benchmarks/benchmark.sh` and recorded in [benchmarks/results.md](benchmarks/results.md). Each repo uses its own Stylelint config. Results vary by machine -- run the script to reproduce on yours.

| Repository | Files | Stylelint | Gale | Speedup |
|------------|------:|----------:|-----:|--------:|
| [GOV.UK Frontend](https://github.com/alphagov/govuk-frontend) | 326 | 4.816s | 0.029s | **166x** |
| [Fundamental Styles](https://github.com/SAP/fundamental-styles) | 392 | 6.800s | 0.121s | **56x** |
| [Mattermost](https://github.com/mattermost/mattermost) | 564 | 6.085s | 0.115s | **53x** |
| [Joomla](https://github.com/joomla/joomla-cms) | 169 | 1.556s | 0.034s | **46x** |
| [freeCodeCamp](https://github.com/freeCodeCamp/freeCodeCamp) | 91 | 0.978s | 0.026s | **38x** |
| [wp-calypso](https://github.com/Automattic/wp-calypso) | 2,051 | 23.167s | 0.676s | **34x** |
| [Bootstrap](https://github.com/twbs/bootstrap) | 99 | 2.798s | 0.088s | **32x** |
| [Angular Components](https://github.com/angular/components) | 627 | 3.028s | 0.112s | **27x** |
| [rsuite](https://github.com/rsuite/rsuite) | 207 | 3.801s | 0.160s | **24x** |
| [Carbon](https://github.com/carbon-design-system/carbon) | 1,263 | 4.389s | 0.195s | **22x** |
| [PatternFly](https://github.com/patternfly/patternfly) | 213 | 6.653s | 0.313s | **21x** |
| [Spectrum CSS](https://github.com/adobe/spectrum-css) | 236 | 4.304s | 0.260s | **17x** |
| [Gutenberg](https://github.com/wordpress/gutenberg) | 729 | 2.835s | 0.172s | **16x** |
| [JupyterLab](https://github.com/jupyterlab/jupyterlab) | 211 | 2.920s | 0.178s | **16x** |
| [Discourse](https://github.com/discourse/discourse) | 377 | 2.899s | 0.191s | **15x** |
| [Grafana](https://github.com/grafana/grafana) | 10 | 0.722s | 0.051s | **14x** |
| [Material UI](https://github.com/mui/material-ui) | 39 | 0.646s | 0.053s | **12x** |
| [Mastodon](https://github.com/mastodon/mastodon) | 36 | 3.216s | 0.268s | **12x** |
| [SLDS](https://github.com/salesforce-ux/design-system) | 446 | 2.641s | 0.229s | **12x** |
| [Primer CSS](https://github.com/primer/css) | 113 | 2.215s | 0.236s | **9x** |
| [Docusaurus](https://github.com/facebook/docusaurus) | 116 | 0.213s | 0.032s | **7x** |

## Parity with Stylelint

Gale's goal is byte-for-byte identical output: every warning Stylelint reports, at the same line and column, with the same text and severity — and nothing extra. Any difference is treated as a bug, not a limitation.

Parity is measured by the [differential harness](tests/differential/) against 22 real-world repositories with **no rule filters** — every warning from both tools is compared. A subset of that corpus runs weekly in CI; current per-repo results (files matched, false positives, false negatives, speedup) are published in [COMPATIBILITY.md](COMPATIBILITY.md). Run the harness yourself to reproduce, or to check a repo the weekly job does not cover. A second harness, [tests/stylelint-compat/](tests/stylelint-compat/), replays the `testRule()` cases from Stylelint's own test suites against Gale to measure per-rule compatibility.

**Where parity currently stands.** Which warnings Gale reports, and at what line, column, rule and severity, tracks Stylelint closely. The **message wording** does not yet: Gale's strings follow Stylelint v15/v16 phrasing, and v17 rewrote many of them. Spot-checking ten common rules against Stylelint 17.14.1 found identical positions and rule IDs on every warning, but different text on most:

| Rule | Stylelint v17 | Gale |
|------|---------------|------|
| `block-no-empty` | `Empty block` | `Unexpected empty block` |
| `color-named` | `Disallowed named color "red"` | `Unexpected named color "red"` |
| `length-zero-no-unit` | `Disallowed unit` | `Unexpected unit` |
| `declaration-block-no-duplicate-properties` | `Duplicate property "color"` | `Unexpected duplicate "color"` |
| `declaration-no-important` | `Disallowed !important` | `Unexpected !important in declaration "color"` |

`color-hex-length` and `color-named` also echo the source's letter case in v17 (`#FFF`, `"RED"`) where Gale lowercases it. If you compare warning text — snapshot tests, a custom reporter — expect differences until the messages are updated.

## Quick start

```bash
# Install
npm install -D @codebend3r/gale

# Lint (uses your existing .stylelintrc)
npx gale "src/**/*.css"

# Autofix
npx gale --fix "src/**/*.css"
```

## Migrate from Stylelint

Change one line in `package.json`:

```diff
 {
   "scripts": {
-    "lint:css": "stylelint 'src/**/*.css'"
+    "lint:css": "gale 'src/**/*.css'"
   }
 }
```

Your `.stylelintrc` stays exactly the same. Gale reads the same config files, follows the same `extends` chains, honors `/* stylelint-disable */` comments, and produces the same JSON output format.

## Installation

### npm (recommended)

```bash
npm install -D @codebend3r/gale
```

The npm package ships prebuilt binaries, so install does not run a postinstall
script or download executables. A small Node launcher picks the right one.
Bundled platforms: macOS (arm64, x64) and Linux (x64, arm64). Windows is not
bundled in the npm package; install it with `cargo install gale-lint` or build
from source.

### Cargo

```bash
cargo install gale-lint
```

The crate is named `gale-lint` on crates.io (since `gale` was taken), but the installed binary is called `gale`.

### From source

```bash
git clone https://github.com/codebend3r/gale.git
cd gale
cargo build --release
# Binary at target/release/gale
```

### GitHub releases

Download pre-built binaries from [GitHub Releases](https://github.com/codebend3r/gale/releases).

## What's supported

For a row-by-row comparison of every feature in Gale and Stylelint, see the
[feature table](docs/features-table.md).

### 271 built-in rules

Gale registers 271 rules across these namespaces:

| Namespace | Count | Examples |
|-----------|------:|---------|
| Core Stylelint | 146 | `block-no-empty`, `color-no-invalid-hex`, `property-no-unknown`, `selector-no-unmatchable` |
| `@stylistic/*` | 69 | `@stylistic/indentation`, `@stylistic/declaration-colon-space-after`, `@stylistic/no-eol-whitespace` |
| `scss/*` | 45 | `scss/at-rule-no-unknown`, `scss/no-duplicate-mixins`, `scss/dollar-variable-pattern` |
| `plugin/*` | 5 | `plugin/enforce-variable-for-property`, `plugin/browser-compat` |
| `order/*` | 3 | `order/order`, `order/properties-order`, `order/properties-alphabetical-order` |
| Vendor plugins | 3 | `csstools/value-no-unknown-custom-properties`, `material/no-prefixes`, `spectrum-tools/no-unknown-custom-properties` |

SCSS, stylistic, order, and plugin rules are built in -- no extra plugins required.

> Stylistic rules use the `@stylistic/` prefix, matching `@stylistic/stylelint-plugin`.

### Config compatibility

Gale walks up from the working directory and uses the first config it finds, in this order:

| File | Format |
|------|--------|
| `gale.json` | JSON (native) |
| `gale.toml` | TOML (native) |
| `.stylelintrc` | JSON or YAML |
| `.stylelintrc.json` | JSON |
| `.stylelintrc.yml` / `.yaml` | YAML |
| `stylelint.config.js` / `.mjs` / `.cjs` | JavaScript |
| `.stylelintrc.js` / `.cjs` / `.mjs` | JavaScript |
| `package.json` (`"stylelint"` field) | JSON (lowest priority) |

> JavaScript configs are **statically parsed**, not executed. Gale reads the exported
> object literal (resolving relative `require`/`import` re-exports and simple scalar
> constants). Configs that compute rules at runtime — loops, function calls, conditionals —
> will not resolve correctly; convert those to JSON or YAML.

### Feature overview

- **CSS, SCSS, and Less** out of the box (no plugins needed)
- **Sass indented syntax** (`.sass`) via an internal Sass-to-SCSS conversion (see caveat below)
- **Autofix** via `--fix`, applied repeatedly until the file stops changing
- **File caching** via `--cache` (skips unchanged files)
- **LSP server** for editor integration (`--lsp`)
- **Parallel linting** using all CPU cores
- **Inline disable comments** (`stylelint-disable` and `gale-disable`)
- **Text, string, JSON, compact, verbose, TAP, and unix** output formatters; JSON matches Stylelint's result shape field-for-field
- **Programmatic Node.js API** (`lint()`, `resolveConfig()`, `formatters`) modeled on `stylelint.lint()`, usable from both ESM and CommonJS
- **`extends`** with built-in presets, npm packages, and relative paths
- **`.stylelintignore` and `.galeignore`** files (gitignore syntax) for custom exclusions

### Declarative plugin rules

Gale includes built-in `plugin/*` meta-rules that cover the most common custom plugin patterns (design token enforcement, custom property analysis, file header checks). These replace the need for JS plugins like `stylelint-plugin-carbon-tokens`, Primer's custom plugins, and `stylelint-copyright`:

| Rule | Description |
|------|-------------|
| `plugin/enforce-variable-for-property` | Enforce design token/variable usage for configured properties |
| `plugin/no-unknown-custom-properties` | Report usage of undefined CSS custom properties |
| `plugin/no-unused-custom-properties` | Report defined but unused CSS custom properties |
| `plugin/require-file-header-comment` | Require a file header comment matching a pattern |
| `plugin/browser-compat` | Report declarations unsupported by the configured browser targets |

### Programmatic API

Importable from ESM (`import`) and CommonJS (`require`) alike, with TypeScript
declarations included.

```javascript
import { lint, resolveConfig, formatters } from '@codebend3r/gale';

const result = await lint({
  files: 'src/**/*.css',
  config: { rules: { 'block-no-empty': true } },
});

console.log(result.errored);        // boolean
console.log(result.results);        // LintResult[]
console.log(result.report);         // formatted string
```

The API shells out to the `gale` binary and reshapes its JSON output. Stylelint
fields Gale does not populate (`deprecations`, `invalidOptionWarnings`,
`parseErrors`, `ruleMetadata`) are present but always empty. `createPlugin()`
exists as a no-op compatibility stub and warns when called.

From CommonJS the async functions work the same way:

```javascript
const { lint } = require('@codebend3r/gale');
```

### Not yet supported

- **Arbitrary JavaScript plugins.** Gale cannot execute JS plugins, but its 271 built-in rules and the `plugin/*` meta-rules cover the vast majority of real-world configs. See [Declarative plugin rules](#declarative-plugin-rules) above.
- **Dynamic JavaScript configs.** See the config compatibility note above.
- **Accurate positions in `.sass` files.** `.sass` sources are converted to SCSS before parsing, so reported line/column numbers refer to the converted text and drift from the original file. Rules fire correctly; the coordinates are not trustworthy.
- **Custom JS formatters.** There is no `--custom-formatter` flag; use one of the built-in formatters.
- **Stylelint v17 message wording.** See [Parity with Stylelint](#parity-with-stylelint) above.

## Configuration

Gale searches for config files walking up from the working directory. To generate a starter config:

```bash
npx gale --init
```

### Example config

```json
{
  "extends": "gale:recommended",
  "rules": {
    "block-no-empty": true,
    "color-hex-length": "warning",
    "number-max-precision": ["error", { "max": 4 }],
    "declaration-no-important": "off"
  }
}
```

### Rule value formats

| Format | Meaning |
|--------|---------|
| `true` | Enable at error severity |
| `false` or `"off"` | Disable |
| `"error"` | Enable at error severity |
| `"warning"` | Enable at warning severity |
| `<primary>` | Stylelint's primary option, e.g. `"number-max-precision": 4` |
| `[<primary>, { secondary }]` | Stylelint's array form, e.g. `["never", { ignore: [...] }]` |
| `[true \| "error" \| "warning", { options }]` | Gale extension: severity first, options second |

Stylelint's per-rule secondary options are honored in every array form:
`severity`, `message` (string form), `url` (surfaced on each JSON warning),
`disableFix` (report but never autofix), and `reportDisables` (report any
`stylelint-disable` comment that names the rule). So is the top-level
`defaultSeverity` field.

### Config-file switches

Every switch below can be set in the config file instead of on the command
line, exactly as in Stylelint. A flag on the command line always wins.

| Config key | Equivalent flag |
|------------|-----------------|
| `"ignoreDisables": true` | `--ignore-disables` |
| `"reportNeedlessDisables": true` | `--report-needless-disables` |
| `"reportInvalidScopeDisables": true` | `--report-invalid-scope-disables` |
| `"reportDescriptionlessDisables": true` | `--report-descriptionless-disables` |
| `"reportUnscopedDisables": true` | `--report-unscoped-disables` |
| `"allowEmptyInput": true` | `--allow-empty-input` |
| `"quiet": true` | `--quiet` |
| `"fix": true` or `"strict"` / `"lax"` | `--fix` / `--fix=lax` |
| `"cache": true` | `--cache` |
| `"cacheLocation": "path"` | `--cache-location path` |
| `"cacheStrategy": "content"` | `--cache-strategy content` |

### Built-in presets

| Preset | Description |
|--------|-------------|
| `gale:recommended` | Sensible defaults (29 rules: 15 error + 14 warning) |
| `gale:all` | Every one of the 271 registered rules at warning severity. This includes the `@stylistic/*` namespace, so expect a lot of formatting noise — it is a discovery tool, not a starting config. |

Gale also has built-in equivalents for `stylelint-config-recommended`,
`stylelint-config-standard`, `stylelint-config-recommended-scss`, and
`stylelint-config-standard-scss`, and can resolve other shareable configs from
`node_modules/`.

### Extends resolution

The `extends` field supports:

| Value | Resolution |
|-------|------------|
| `"gale:recommended"` | Built-in preset |
| `"gale:all"` | Built-in preset |
| `"./path/to/config.json"` | Relative path to another config file |
| `"stylelint-config-standard"` | npm package (resolved from `node_modules/`) |

Resolution is recursive with cycle detection. Later `extends` entries override earlier ones. User `rules` always override extended rules.

## CLI reference

```
gale [OPTIONS] [FILES]...
```

| Flag | Description |
|------|-------------|
| `<files>` | Files, directories, or glob patterns to lint |
| `--fix` | Automatically fix problems (default: strict — skips files with parse errors) |
| `--fix=lax` | Also fix files that have parse errors |
| `-q, --quiet` | Only report errors |
| `-f, --formatter <type>` | Output: `text` (default), `string`, `json`, `compact`, `verbose`, `tap`, `unix`. An unknown value is rejected |
| `-c, --config <path>` | Config file path |
| `--max-warnings <n>` | Error if warnings exceed threshold |
| `--cache` | Skip unchanged files |
| `--cache-location <path>` | Custom cache file path (default: `.gale_cache`) |
| `--cache-strategy <name>` | `metadata` (default: mtime and size) or `content` (hash) decides what counts as unchanged |
| `--stdin` | Read from stdin |
| `--stdin-filename <name>` | Virtual filename for stdin (default: `stdin.css`) |
| `--allow-empty-input` | Don't error when no files match |
| `--ignore-path <file>` | Custom ignore file (gitignore syntax) |
| `--ignore-pattern <glob>`, `--ip` | Extra ignore glob, on top of the ignore files (repeatable) |
| `--disable-default-ignores`, `--di` | Lint `node_modules` too instead of always skipping it |
| `--no-ignore` | Disable all ignore file processing |
| `--ignore-disables` | Ignore all `stylelint-disable` comments |
| `--report-needless-disables` | Report disable comments that suppress nothing |
| `--report-invalid-scope-disables` | Report disable comments for rules not being linted |
| `--report-descriptionless-disables` | Report disable comments without a description |
| `--report-unscoped-disables` | Report disable comments that name no rule |
| `--color` / `--no-color` | Force colour on or off in the text and verbose formatters |
| `--custom-syntax <name>` | Parse every file as `postcss`, `postcss-scss`, `postcss-less` or `postcss-sass`; any other syntax skips every file |
| `-o, --output-file <path>` | Write the report to a file (colour stripped) as well as printing it |
| `--quiet-deprecation-warnings` | Accepted for compatibility; Gale emits no deprecation warnings |
| `--print-config <file>` | Print resolved config as JSON |
| `--init` | Generate starter config |
| `--lsp` | Start LSP server |
| `-V, --version` | Print version |

Exit codes match Stylelint's:

| Code | Meaning |
|-----:|---------|
| `0` | No error-severity problems |
| `1` | Fatal error, including no files matching the patterns (pass `--allow-empty-input` to make an empty match succeed) |
| `2` | Error-severity problems found, or `--max-warnings` exceeded |
| `64` | Invalid command line, such as an unknown flag or formatter |
| `78` | A config file that exists but cannot be loaded |

Colour in the `text` and `verbose` formatters follows the same rule as Stylelint:
`NO_COLOR` or `--no-color` turn it off; otherwise `FORCE_COLOR`, `--color`, or `CI`
turn it on; otherwise it is on only when stdout is a terminal.

## Editor integration

### Any LSP-compatible editor

```bash
gale --lsp
```

Works with Neovim, Helix, Zed, and any editor supporting the Language Server Protocol.
Every fixable diagnostic is offered as a quick-fix code action.

### VS Code

The extension lives in its own repo, [codebend3r/gale-plugin](https://github.com/codebend3r/gale-plugin).
It is not published to the Marketplace — build a `.vsix` locally:

```bash
git clone https://github.com/codebend3r/gale-plugin.git
cd gale-plugin
bun install
bun run compile   # tsc -p ./
bun run package   # vsce package
```

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution workflow and the
steps for adding a rule.

### Prerequisites

- Rust 1.85+ (2024 edition)
- Python 3 (for differential tests)
- Node.js 20+ (for the npm package and the API tests; `.nvmrc` pins 26, and CI tests 20, 22, 24 and 26)
- [Bun](https://bun.sh) (runs the repo scripts and the feature test suite)

Install the JavaScript dev dependencies once:

```bash
bun install
```

### Build and test

```bash
cargo build                          # Debug build
cargo build --release                # Release build
cargo test --workspace               # Run all tests
cargo test -p gale_linter            # Tests for a specific crate
cargo test -p gale_linter block_no_empty  # A specific test
cargo clippy --workspace -- -D warnings   # Lint the Rust code
cargo fmt --check                    # Check formatting
```

### Run the linter

```bash
cargo run -- "src/**/*.css"          # Lint
cargo run -- --fix "src/**/*.css"    # Autofix
cargo run -- --formatter json src/   # JSON output
```

### Debug and profiling

```bash
GALE_DEBUG_PERF=1 cargo run --release -- src/   # Per-phase timings to stderr
GALE_LOG=debug cargo run -- src/                # Tracing/logging output
```

### Differential testing

Compare Gale output against Stylelint on real-world repositories:

```bash
python tests/differential/run.py              # All repos
python tests/differential/run.py bootstrap    # Specific repo
python tests/differential/run.py --benchmark  # Include timing comparison
python tests/differential/run.py --list       # List available repos
python tests/differential/run.py --css-only   # Skip SCSS/Less
python tests/differential/run.py --skip-build # Use existing binary
python tests/differential/run.py --gale-bin path/to/gale  # Use a specific binary
python tests/differential/run.py --update     # Force re-clone repos
```

The test corpus includes Bootstrap, Gutenberg, Carbon, Angular Components, wp-calypso, Discourse, GOV.UK Frontend, Spectrum CSS, Docusaurus, Grafana, Material UI, freeCodeCamp, PatternFly, Primer CSS, Elastic EUI, Mattermost, Mastodon, JupyterLab, Joomla, SLDS, rsuite, and Fundamental Styles.

### Benchmarks

```bash
bash benchmarks/benchmark.sh         # Full benchmark suite
bash benchmarks/benchmark-quick.sh   # Quick benchmark (Bootstrap CSS, 1x and 20x)
```

Both scripts download fixtures on first run and install a local Stylelint with
`bun`. `benchmark-quick.sh` uses `hyperfine` when available and falls back to
`time` otherwise.

### Test suites

```bash
bun run test            # Rust tests, npm API tests, and feature tests
bun run test:rust       # cargo test across the workspace
bun run test:api        # npm/test.mjs against target/release/gale
bun run test:features   # bun test tests/features
bun run system-check    # fmt, clippy, release build, npm build, and every suite
```

`test:api` and `test:features` run the release binary, so build it first with
`cargo build --release`. The API tests exercise `lint()`, `formatters`, and
`resolveConfig()` from both the ESM and CommonJS entry points. The feature
tests in [tests/features/](tests/features/) drive the binary end to end inside
throwaway projects, one file per feature area from the
[feature table](docs/features-table.md). CI runs all three on every pull
request.

## Releasing

Releases are automated via GitHub Actions when you push a version tag. See
[PUBLISHING.md](PUBLISHING.md) for the package layout, the manual publish path,
and troubleshooting.

```bash
# 1. Update the version in Cargo.toml (workspace.package.version)
# 2. Commit the version bump
# 3. Tag and push
git tag v0.2.3
git push && git push --tags
```

The [release workflow](.github/workflows/release.yml) will:

1. Build binaries for Linux (x64, arm64), macOS (x64, arm64), and Windows (x64)
2. Create a GitHub Release with the binaries
3. Stage those binaries inside `npm/bin/<target>/`
4. Publish the npm package (`@codebend3r/gale`) with the matching version
5. Publish the `gale-lint` crate to crates.io

### Manual npm build

```bash
# Build and stage the current platform binary in npm/bin/<target>/gale
./scripts/build-npm.sh

# Build and stage all supported binaries (requires cross + Docker)
./scripts/build-npm.sh --all

# Set npm package version before building
./scripts/build-npm.sh --version 0.2.3
```

## Architecture

Gale is organized as a Cargo workspace with seven crates:

```
gale (binary)
  |
  v
gale_cli         CLI definition (clap), file discovery, orchestration
  |
  +-- gale_config       Config loading, resolution, presets
  +-- gale_linter       Rule trait, registry, runner, 271 built-in rules
  |     +-- gale_css_parser    CSS/SCSS/Less parser (lightningcss + raffia)
  |     +-- gale_diagnostics   Span, Diagnostic, LintResult, Fix/Edit types
  +-- gale_formatter    Output formatters (text, json, compact, verbose, tap, unix)
  +-- gale_lsp          Language Server Protocol server
```

| Crate | Responsibility |
|-------|----------------|
| `gale_css_parser` | Wraps **lightningcss** (CSS) and **raffia** (SCSS/Less) into a unified, owned AST |
| `gale_diagnostics` | Core types: `Span`, `Diagnostic`, `LintResult`, `Fix`, `Edit` |
| `gale_linter` | `Rule` trait, `RuleRegistry`, `LintRunner`, inline disable comments, all rule implementations |
| `gale_config` | Config file discovery, parsing (JSON/YAML/TOML/JS), `extends` resolution, built-in presets |
| `gale_formatter` | `TextFormatter`, `JsonFormatter`, `CompactFormatter`, `VerboseFormatter`, `TapFormatter`, `UnixFormatter` |
| `gale_cli` | Clap CLI, file discovery with ignore support, cache layer, `--fix` orchestration |
| `gale_lsp` | LSP server for real-time editor diagnostics |

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to add a rule.

## License

[MIT](LICENSE)
