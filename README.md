# Gale

**An extremely fast CSS linter. Drop-in replacement for Stylelint.**

[![npm version](https://img.shields.io/npm/v/@codebend3r/gale)](https://www.npmjs.com/package/@codebend3r/gale)
[![CI](https://github.com/LyricalString/gale/actions/workflows/ci.yml/badge.svg)](https://github.com/LyricalString/gale/actions)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Gale reads your existing `.stylelintrc`, runs the same rules, and produces the same output — typically **20-100x faster** on real projects.

One line change in your `package.json`. No config migration.

> **Compatibility note:** Gale targets **Stylelint v17** semantics. If your project uses Stylelint v16 or earlier, you may see minor differences in edge cases (e.g., how `selector-max-*` rules count selectors inside `:is()`, `:has()`, `:where()`). These match the behavior you would get after upgrading to Stylelint v17.

## Benchmarks

Real-world benchmarks using [hyperfine](https://github.com/sharkdp/hyperfine) (10 runs, 3 warmup) on an Apple M4 Max. Each repo uses its own Stylelint config. Results vary by machine -- run `./benchmarks/benchmark.sh` to reproduce on yours.

| Repository | Files | Stylelint | Gale | Speedup |
|------------|------:|----------:|-----:|--------:|
| [Angular Components](https://github.com/angular/components) | 621 | 0.743s | 0.008s | **96x** |
| [Fundamental Styles](https://github.com/SAP/fundamental-styles) | 392 | 3.628s | 0.060s | **61x** |
| [GOV.UK Frontend](https://github.com/alphagov/govuk-frontend) | 149 | 2.352s | 0.044s | **54x** |
| [Discourse](https://github.com/discourse/discourse) | 356 | 0.529s | 0.010s | **51x** |
| [Joomla](https://github.com/joomla/joomla-cms) | 169 | 1.033s | 0.025s | **41x** |
| [Carbon](https://github.com/carbon-design-system/carbon) | 1,116 | 0.385s | 0.010s | **38x** |
| [Bootstrap](https://github.com/twbs/bootstrap) | 99 | 0.737s | 0.021s | **35x** |
| [Gutenberg](https://github.com/wordpress/gutenberg) | 778 | 0.447s | 0.013s | **34x** |
| [PatternFly](https://github.com/patternfly/patternfly) | 204 | 0.377s | 0.013s | **29x** |
| [SLDS](https://github.com/salesforce-ux/design-system) | 446 | 0.323s | 0.014s | **24x** |

## Parity with Stylelint

Gale's goal is byte-for-byte identical output: every warning Stylelint reports, at the same line and column, with the same text and severity — and nothing extra. Any difference is treated as a bug, not a limitation.

Parity is measured by the [differential harness](tests/differential/) against 22 real-world repositories with **no rule filters** — every warning from both tools is compared. A subset of that corpus runs weekly in CI; current per-repo results (files matched, false positives, false negatives, speedup) are published in [COMPATIBILITY.md](COMPATIBILITY.md). Run the harness yourself to reproduce, or to check a repo the weekly job does not cover.

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
script or download executables. A small `bin/gale` shell wrapper picks the right
one. Supported platforms: macOS (arm64, x64), Linux (x64, arm64).

### Cargo

```bash
cargo install gale-lint
```

The crate is named `gale-lint` on crates.io (since `gale` was taken), but the installed binary is called `gale`.

### From source

```bash
git clone https://github.com/LyricalString/gale.git
cd gale
cargo build --release
# Binary at target/release/gale
```

### GitHub releases

Download pre-built binaries from [GitHub Releases](https://github.com/LyricalString/gale/releases).

## What's supported

### 269 built-in rules

Gale registers 269 rules across these namespaces:

| Namespace | Count | Examples |
|-----------|------:|---------|
| Core Stylelint | 144 | `block-no-empty`, `color-no-invalid-hex`, `property-no-unknown`, `display-notation` |
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
- **Text, JSON, compact, verbose, TAP, and unix** output formatters; JSON matches Stylelint's result shape field-for-field
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

Importable from ESM (`import`) and CommonJS (`require`) alike.

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

- **Arbitrary JavaScript plugins.** Gale cannot execute JS plugins, but its 269 built-in rules and the `plugin/*` meta-rules cover the vast majority of real-world configs. See [Declarative plugin rules](#declarative-plugin-rules) above.
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

Stylelint's per-rule `{ "severity": "warning" }` secondary option is honored in
every array form, as is the top-level `defaultSeverity` field.

### Built-in presets

| Preset | Description |
|--------|-------------|
| `gale:recommended` | Sensible defaults (29 rules: 15 error + 14 warning) |
| `gale:all` | Every one of the 269 registered rules at warning severity. This includes the `@stylistic/*` namespace, so expect a lot of formatting noise — it is a discovery tool, not a starting config. |

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
| `--stdin` | Read from stdin |
| `--stdin-filename <name>` | Virtual filename for stdin (default: `stdin.css`) |
| `--allow-empty-input` | Don't error when no files match |
| `--ignore-path <file>` | Custom ignore file (gitignore syntax) |
| `--no-ignore` | Disable all ignore file processing |
| `--ignore-disables` | Ignore all `stylelint-disable` comments |
| `--report-needless-disables` | Report disable comments that suppress nothing |
| `--report-invalid-scope-disables` | Report disable comments for rules not being linted |
| `--report-descriptionless-disables` | Report disable comments without a description |
| `--print-config <file>` | Print resolved config as JSON |
| `--init` | Generate starter config |
| `--lsp` | Start LSP server |
| `-V, --version` | Print version |

Exit code is `1` when any error-severity problem is found, when `--max-warnings`
is exceeded, or when no files match the given patterns (pass `--allow-empty-input`
to make an empty match succeed). `0` otherwise. This matches Stylelint.

## Editor integration

### Any LSP-compatible editor

```bash
gale --lsp
```

Works with Neovim, Helix, Zed, and any editor supporting the Language Server Protocol.

### VS Code

An extension lives in [`editors/vscode/`](editors/vscode). It is not published to the
Marketplace — build a `.vsix` locally:

```bash
cd editors/vscode
npm install
npm run compile   # tsc -p ./
npm run package   # vsce package
```

## Development

### Prerequisites

- Rust 1.85+ (2024 edition)
- Python 3 (for differential tests)
- Node.js 16+ (for differential tests and npm packaging)

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
python tests/differential/run.py --update     # Force re-clone repos
```

The test corpus includes Bootstrap, Gutenberg, Carbon, Angular Components, wp-calypso, Discourse, GOV.UK Frontend, Spectrum CSS, Docusaurus, Grafana, Material UI, freeCodeCamp, PatternFly, Primer CSS, Elastic EUI, Mattermost, Mastodon, JupyterLab, Joomla, SLDS, rsuite, and Fundamental Styles.

### Benchmarks

```bash
bash benchmarks/benchmark.sh         # Full benchmark suite
bash benchmarks/run-benchmark.sh     # Quick benchmark (Bootstrap CSS, 1x and 20x)
```

Both scripts download fixtures on first run and install a local Stylelint with
`bun`. `run-benchmark.sh` uses `hyperfine` when available and falls back to
`time` otherwise.

### npm package smoke tests

```bash
npm test          # from the repo root
# or:
cd npm && npm test
```

Exercises `lint()`, `formatters`, and `resolveConfig()` against the built binary
from both the ESM and CommonJS entry points. CI runs it on every push.

## Releasing

Releases are automated via GitHub Actions when you push a version tag:

```bash
# 1. Update the version in Cargo.toml (workspace.package.version)
# 2. Commit the version bump
# 3. Tag and push
git tag v0.2.0
git push && git push --tags
```

The [release workflow](.github/workflows/release.yml) will:

1. Build binaries for Linux (x64, arm64) and macOS (x64, arm64)
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
./scripts/build-npm.sh --version 0.2.0
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
  +-- gale_linter       Rule trait, registry, runner, 269 built-in rules
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

See [CLAUDE.md](CLAUDE.md) for detailed architecture docs and how to add rules.

## License

[MIT](LICENSE)
