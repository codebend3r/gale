# Gale -- Technical Guide

This document is a reference for AI assistants and developers working on the Gale codebase.

## Project overview

Gale is a **perfect substitute for [Stylelint](https://stylelint.io/)**, written in Rust. Not an alternative — a **drop-in replacement** that produces **exactly the same output**, just 100-400x faster.

The bar: `bunx gale 'src/**/*.scss'` must produce **byte-for-byte identical warnings** to `bunx stylelint 'src/**/*.scss'`. Same rules, same config, same warnings, same severity, same line/column. If Stylelint says it, Gale says it. If Stylelint doesn't say it, Gale doesn't say it. Zero false positives, zero false negatives.

**Current state (v0.1.5):**
- 270+ built-in rules (146 core, 44 SCSS, 64 stylistic, 3 order, 4 plugin meta-rules)
- Targets **Stylelint v17** semantics
- Differential tested against 22 real-world repos with ZERO rule filters
- 6 output formatters (text, json, compact, verbose, tap, unix) + `--custom-formatter`
- Programmatic Node.js API (`lint()`, `resolveConfig()`, `formatters`)
- Published on npm as `@codebend3r/gale`
- Published on crates.io as `gale-lint` (binary name is `gale`)

## Build and test commands

```sh
# Build
cargo build                          # debug
cargo build --release                # release

# Test
cargo test --workspace               # all tests
cargo test -p gale_linter            # specific crate
cargo test -p gale_linter block_no_empty  # specific test

# Lint the Rust code
cargo clippy --workspace -- -D warnings
cargo fmt --check

# Every linter check CI runs, in one command (needs bun; run `bun install` once)
bun run system-check
# ...or any single step: fmt:check, lint, test:rust, build:release,
# test:api, test:features
bun run lint

# Run Gale
cargo run -- "src/**/*.css"          # lint
cargo run -- --fix "src/**/*.css"    # autofix
cargo run -- --formatter json src/   # JSON output

# Debug
GALE_DEBUG_PERF=1 cargo run --release -- src/   # per-phase timings
GALE_LOG=debug cargo run -- src/                # tracing output

# Benchmarks
bash benchmarks/benchmark.sh
bash benchmarks/run-benchmark.sh

# Differential testing against Stylelint
python tests/differential/run.py                    # all repos
python tests/differential/run.py bootstrap          # specific repo
python tests/differential/run.py --benchmark        # with timing
python tests/differential/run.py bootstrap --css-only  # skip SCSS/Less
python tests/differential/run.py --skip-build       # use existing binary
```

## Architecture

### Crate dependency graph

```
src/main.rs  (binary entrypoint -- delegates to gale_cli::run())
    |
    v
gale_cli        CLI definition (clap), file discovery, orchestration
    |
    +-- gale_config       Config loading, resolution, presets
    +-- gale_linter       Rule trait, registry, runner, built-in rules
    |       +-- gale_css_parser    CSS/SCSS/Less parser wrapper
    |       +-- gale_diagnostics   Span, Diagnostic, LintResult, Fix/Edit types
    +-- gale_formatter    Output formatters (text, json, compact, verbose, tap, unix)
    +-- gale_lsp          Language Server Protocol server
```

### Data flow

```
CLI args
  -> resolve config (find_config -> load_config -> resolve extends/presets)
  -> discover CSS files (ignore + WalkBuilder)
  -> for each file (in parallel via rayon):
       parse(source, syntax) -> ParseResult { nodes: Vec<CssNode> }
       for each enabled rule:
         rule.check_root(nodes, context) -> Vec<Diagnostic>
         walk AST: rule.check(node, context) -> Vec<Diagnostic>
       filter by inline disable comments
       sort diagnostics by offset
  -> apply fixes if --fix
  -> format output (text/json/compact)
  -> exit code 1 if errors found
```

### Crate-by-crate breakdown

| Crate | Path | Responsibility |
|-------|------|----------------|
| **gale_css_parser** | `crates/gale_css_parser/` | Wraps **lightningcss** (CSS) and **raffia** (SCSS/Less) into a unified, lifetime-free AST (`CssNode` enum). Handles syntax detection from file extensions. |
| **gale_diagnostics** | `crates/gale_diagnostics/` | Core types: `Span`, `SourceLocation`, `SourceLineIndex`, `Severity`, `Diagnostic`, `LintResult`, `Fix`, `Edit`. Also provides `apply_fixes()`. |
| **gale_linter** | `crates/gale_linter/` | The `Rule` trait, `RuleRegistry`, `LintRunner`, inline disable-comment processing, known-identifier data tables, and all 250 built-in rule implementations. |
| **gale_config** | `crates/gale_config/` | Config file discovery (walks up directories), parsing (JSON/YAML/TOML), `extends` resolution (built-in presets, npm packages, relative paths), preset definitions (`gale:recommended`, `gale:all`). |
| **gale_formatter** | `crates/gale_formatter/` | `Formatter` trait with `TextFormatter` (Stylelint-like), `JsonFormatter` (Stylelint-compatible JSON), `CompactFormatter`, `VerboseFormatter`, `TapFormatter`, and `UnixFormatter`. Factory function `create_formatter()`. |
| **gale_cli** | `crates/gale_cli/` | Clap-based CLI, file discovery with `.galeignore`/`.gitignore` support, cache layer, `--fix` orchestration, `--init` scaffolding, `--lsp` delegation. |
| **gale_lsp** | `crates/gale_lsp/` | LSP server for editor integration. Invoked via `gale --lsp`. |

## File structure

```
gale/
  Cargo.toml              Workspace root (version 0.1.1, Rust 2024 edition)
  src/main.rs             Binary entrypoint (delegates to gale_cli::run)
  crates/
    gale_cli/             CLI, file discovery, orchestration, caching
    gale_config/          Config loading, parsing, extends resolution, presets
    gale_css_parser/      lightningcss + raffia wrapper, CssNode AST
    gale_diagnostics/     Span, Diagnostic, LintResult, Fix/Edit, apply_fixes
    gale_formatter/       Text/JSON/Compact output formatters
    gale_linter/          Rule trait, registry, runner, built-in rules
      src/
        rule.rs           Rule trait + RuleContext
        registry.rs       RuleRegistry
        runner.rs         LintRunner + inline disable comment processing
        data.rs           Known CSS identifiers (properties, units, etc.)
        rules/
          mod.rs          Module declarations + register_all()
          block_no_empty.rs
          scss_at_rule_no_unknown.rs
          stylistic_indentation.rs
          order_properties_order.rs
          ... (250 rule files)
    gale_lsp/             LSP server
  npm/
    package.json          npm package (@codebend3r/gale)
    install.js            Post-install script (downloads platform binary from GitHub releases)
    bin/                  Binary placeholder
    README.md             npm page README
  benchmarks/
    benchmark.sh          Full benchmark suite
    run-benchmark.sh      Quick benchmark (uses hyperfine)
    generate-benchmark.sh Generate test fixtures
    results.md            Latest benchmark results
  tests/
    differential/
      run.py              Differential testing harness
      repos.json          Test corpus (16 repos)
      migration_test.py   Migration testing
  scripts/
    build-npm.sh          Build and package binaries for npm
    generate-compatibility-matrix.py  Generate compatibility report from CI
  .github/workflows/
    ci.yml                Tests, clippy, fmt, benchmark on push/PR
    release.yml           Build binaries + create GitHub release + publish npm on tag push
    compatibility.yml     Weekly differential tests on 4 repos (Monday 6am UTC)
```

## Key types and traits

### `Rule` trait (`gale_linter::rule`)

```rust
pub trait Rule: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn default_severity(&self) -> Severity;

    // Per-node check (called for every AST node during tree walk)
    fn check(&self, node: &CssNode, context: &RuleContext) -> Vec<Diagnostic>;

    // Document-level check (called once with all top-level nodes)
    fn check_root(&self, nodes: &[CssNode], context: &RuleContext) -> Vec<Diagnostic>;
}
```

Most rules implement `check()` for per-node inspection. Rules that need cross-node context (e.g., duplicate detection, source-level scans) implement `check_root()` instead.

### `RuleContext`

```rust
pub struct RuleContext<'a> {
    pub file_path: &'a str,
    pub source: &'a str,
    pub syntax: Syntax,
}
```

### `CssNode` (simplified AST)

```rust
pub enum CssNode {
    Style(StyleRule),    // selector + declarations + nested children
    AtRule(AtRule),       // @media, @keyframes, etc. + children
    Comment(Comment),    // /* text */
    Declaration(Declaration),  // property: value
}
```

All AST types are **owned** (no lifetimes) and `Serialize`/`Deserialize`. Each has a `Span { offset, length }` for byte-offset positions.

### `Diagnostic`

```rust
pub struct Diagnostic {
    pub rule_name: String,
    pub message: String,
    pub severity: Severity,     // Error, Warning, Info, Hint
    pub span: Span,             // byte offset + length
    pub file_path: String,
    pub fix: Option<Fix>,       // optional auto-fix
}
```

Builder pattern: `Diagnostic::new(rule, msg).severity(s).span(sp).fix(f)`

### Other key types

- **`LintResult`** -- `{ file_path, diagnostics, source }` -- output of linting one file.
- **`GaleConfig`** -- `{ rules: HashMap<String, RuleConfig>, ignore_patterns, formatter }` -- resolved config.
- **`RuleConfig`** -- `{ severity, options }` -- per-rule configuration.
- **`RuleRegistry`** -- stores `Vec<Box<dyn Rule>>`, created via `RuleRegistry::default()` (calls `register_all()`).
- **`LintRunner`** -- holds a registry + list of enabled rule names. `lint_source(source, path, syntax) -> LintResult`.

## How to add a new rule

### Step 1: Create the rule file

Create `crates/gale_linter/src/rules/your_rule_name.rs`:

```rust
use gale_css_parser::CssNode;
use gale_diagnostics::{Diagnostic, Severity, Span};

use crate::rule::{Rule, RuleContext};

pub struct YourRuleName;

impl Rule for YourRuleName {
    fn name(&self) -> &'static str {
        "your-rule-name"  // kebab-case, matching Stylelint's name
    }

    fn description(&self) -> &'static str {
        "Disallow something bad"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning  // or Severity::Error
    }

    fn check(&self, node: &CssNode, _context: &RuleContext) -> Vec<Diagnostic> {
        match node {
            CssNode::Style(rule) => {
                // Inspect the node, return diagnostics
                vec![]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gale_css_parser::{Syntax, /* other types as needed */};

    fn ctx() -> RuleContext<'static> {
        RuleContext { file_path: "test.css", source: "", syntax: Syntax::Css }
    }

    #[test]
    fn test_detects_problem() {
        let rule = YourRuleName;
        // construct a CssNode, call rule.check(), assert diagnostics
    }
}
```

### Step 2: Register the module

Add to `crates/gale_linter/src/rules/mod.rs`:

```rust
pub mod your_rule_name;
```

### Step 3: Register in `register_all()`

In the same file (`mod.rs`), add to the `register_all()` function:

```rust
registry.register(Box::new(your_rule_name::YourRuleName));
```

### Step 4 (if applicable): Add to config presets

If the rule should be part of `gale:recommended` or `gale:all`, update the appropriate constant in `crates/gale_config/src/lib.rs`:

- `ALL_RULE_NAMES` -- must be updated for any new rule
- `RECOMMENDED_ERROR_RULES` or `RECOMMENDED_WARNING_RULES` -- if the rule should be in `gale:recommended`

### Per-node vs. document-level rules

- **`check(node, ctx)`** -- called for every node during AST walk. Use for rules that inspect individual declarations, selectors, or at-rules.
- **`check_root(nodes, ctx)`** -- called once with all top-level nodes. Use for rules that need cross-node context like duplicate detection.

### Rules with auto-fix

Return a `Fix` on the `Diagnostic`:

```rust
use gale_diagnostics::{Edit, Fix};

Diagnostic::new(self.name(), "message")
    .span(span)
    .fix(Fix::new("description", vec![
        Edit::new(Span::new(offset, length), "replacement text"),
    ]))
```

## Config system

### Config file search order

`find_config()` walks up from the working directory, checking these names in order:

1. `gale.json`
2. `gale.toml`
3. `.stylelintrc`
4. `.stylelintrc.json`
5. `.stylelintrc.yml`
6. `.stylelintrc.yaml`

Note: `stylelint.config.js` and `.cjs` are also supported.

### Rule value formats (matching Stylelint)

| Format | Meaning |
|--------|---------|
| `true` | Enable at error severity |
| `false` | Disable |
| `"error"` | Enable at error severity |
| `"warning"` | Enable at warning severity |
| `"off"` | Disable |
| `["error", { options }]` | Enable with options |

### Extends resolution

| Value | Resolution |
|-------|------------|
| `"gale:recommended"` | Built-in preset: 15 error rules + 14 warning rules |
| `"gale:all"` | Built-in preset: all rules at warning severity |
| `"./path/to/config.json"` | Relative path to another config file |
| `"stylelint-config-standard"` | npm package (looks in `node_modules/`) |

Resolution is recursive with cycle detection. Later `extends` entries override earlier ones. User `rules` always override extended rules.

### Built-in presets

**`gale:recommended`** enables:

- **Error rules** (15): `block-no-empty`, `color-no-invalid-hex`, `declaration-block-no-duplicate-properties`, `declaration-block-no-duplicate-custom-properties`, `font-family-no-duplicate-names`, `no-duplicate-at-import-rules`, `no-duplicate-selectors`, `no-empty-source`, `property-no-unknown`, `selector-pseudo-class-no-unknown`, `selector-pseudo-element-no-unknown`, `selector-type-no-unknown`, `unit-no-unknown`, `no-descending-specificity`, `keyframe-block-no-duplicate-selectors`

- **Warning rules** (14): `color-hex-length`, `color-hex-case`, `length-zero-no-unit`, `declaration-no-important`, `selector-pseudo-element-colon-notation`, `no-invalid-double-slash-comments`, `function-name-case`, `shorthand-property-no-redundant-values`, `at-rule-no-vendor-prefix`, `property-no-vendor-prefix`, `value-no-vendor-prefix`, `value-keyword-case`, `function-url-quotes`, `number-max-precision`

## Inline disable comments

Both `gale-` and `stylelint-` prefixes are supported:

```css
/* gale-disable */            /* disable all rules until enable */
/* gale-enable */             /* re-enable all rules */
/* gale-disable rule-name */  /* disable specific rule */
/* gale-enable rule-name */   /* re-enable specific rule */
/* gale-disable-next-line */  /* disable all rules on the next line only */
/* gale-disable-next-line rule-name */  /* disable specific rule on next line */

/* stylelint-disable */       /* also works (compatibility) */
```

## Deployment

### CI/CD (GitHub Actions)

Three workflows:

1. **`ci.yml`** -- Runs on push/PR to `main`. Jobs: test, clippy, fmt check, benchmark.
2. **`release.yml`** -- Triggered by `v*` tags. Builds Linux binaries (x64 + arm64 via cross), creates a GitHub Release, publishes the npm package.
3. **`compatibility.yml`** -- Weekly (Monday 6am UTC) or manual. Runs differential tests against Bootstrap, Gutenberg, Grafana, and Primer CSS. Commits an updated `COMPATIBILITY.md`.

### npm package

The npm package (`@codebend3r/gale`) uses a postinstall script (`npm/install.js`) that downloads the correct platform binary from GitHub Releases. Supported platforms: `darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64`.

The `scripts/build-npm.sh` script handles building the binary and copying it into `npm/bin/`.

### crates.io

The crate is published as `gale-lint` on crates.io (the name `gale` was taken). The binary name is `gale`. Install via `cargo install gale-lint`.

### Releasing a new version

1. Update `workspace.package.version` in `Cargo.toml`
2. Commit and tag: `git tag v0.X.Y`
3. Push: `git push && git push --tags`
4. The release workflow handles everything else

## Differential testing

The differential testing harness (`tests/differential/`) validates Gale as a **perfect substitute** by comparing its output against Stylelint on 20 real-world repositories. The comparison is **completely unfiltered** — every warning from both tools is compared, with zero exceptions.

### Philosophy

Stylelint is the source of truth. If Stylelint reports a warning, Gale must report it too. If Stylelint doesn't report it, Gale must not report it. There are no "known gaps", no "unsupported rules", no filtered comparisons. Any discrepancy is a bug that must be fixed.

### How it works

1. Shallow-clones repos from `tests/differential/repos.json`
2. Installs deps (auto-detects npm/yarn/pnpm/bun)
3. Runs `stylelint --formatter json <globs>` using the repo's own config
4. Runs `gale --formatter json <globs>` using the same config and same globs
5. Compares ALL warnings by `(line, column, rule, severity, text)` — **no rule filters**
6. Reports exact FN/FP counts per rule and per file

Both tools receive the **same glob patterns** (defined in `repos.json`), so they discover files identically. This mirrors the real drop-in workflow: `bunx stylelint 'src/**/*.scss'` → `bunx gale 'src/**/*.scss'`.

### Test corpus (20 repos)

Bootstrap, Gutenberg, Carbon, Angular Components, wp-calypso, Discourse, GOV.UK Frontend, Spectrum CSS, Docusaurus, Grafana, Material UI, freeCodeCamp, PatternFly, Primer CSS, Elastic EUI, Mattermost, Mastodon, JupyterLab, Polaris, Joomla, and SLDS.

### Key metrics

- **FN (False Negatives)** = Stylelint reports but Gale misses → bug in Gale
- **FP (False Positives)** = Gale reports but Stylelint does not → bug in Gale
- **Target: 0 FN + 0 FP on every repo**

## Key decisions and constraints

### Parser: lightningcss + raffia

- **lightningcss** for CSS parsing -- extremely fast, production-grade. Uses `ParserFlags::NESTING` and `error_recovery: true`.
- **raffia** for SCSS and Less parsing -- different AST converted to the same `CssNode` representation.
- The parser produces an **owned, lifetime-free AST** (`CssNode` enum) so nodes can be stored, cloned, and serialized freely.
- `Span` uses **byte offsets** (not line/column). Line/column conversion via `SourceLineIndex` uses binary search for O(log n) lookups.

### Performance

- Files are linted in parallel using `rayon::par_iter()`
- File discovery uses the `ignore` crate (same as ripgrep) for fast directory walking
- `SourceLineIndex` provides O(log n) offset-to-line conversion via binary search
- Optional file caching skips unchanged clean files
- `GALE_DEBUG_PERF=1` emits per-phase timing to stderr

### Output compatibility

- JSON output format matches Stylelint's exactly (array of `{source, warnings}` objects)
- Text output format mimics Stylelint's string formatter
- Exit codes match: 0 for clean, 1 for errors

## Code conventions

- **Rule struct names** are PascalCase versions of the kebab-case rule name (e.g., `block-no-empty` -> `BlockNoEmpty`)
- **Rule files** use snake_case matching the struct name (e.g., `block_no_empty.rs`)
- SCSS rules are prefixed `scss_` (e.g., `scss_at_rule_no_unknown.rs`), stylistic rules `stylistic_`, order rules `order_`
- Every rule file includes `#[cfg(test)] mod tests` with unit tests
- The `data.rs` module in `gale_linter` contains sorted arrays of known CSS identifiers (properties, at-rules, pseudo-classes, pseudo-elements, units) for validation rules, using case-insensitive binary search
- Diagnostics use the builder pattern: `Diagnostic::new(name, msg).severity(s).span(sp)`
- Spans always use **byte offsets** from the start of the source
- The codebase uses Rust 2024 edition
- Tracing is controlled via `GALE_LOG` env var
- File ignore supports `.galeignore` files (gitignore syntax) in addition to `.gitignore`

## Target compatibility

Gale targets **Stylelint v17** (the latest major version). Key v17 changes implemented:

- `selector-max-*` rules lint selectors as-written (no desugaring of CSS nesting or functional pseudo-classes)
- `*-list` rules use strict matching (no implicit vendor-prefix or case-insensitive matching)
- `*-no-vendor-prefix` ignore options match as-is (no prefix stripping)
- `&` nesting selector specificity uses `:is()` semantics
- `--fix` defaults to strict mode (use `--fix=lax` for old behavior)
- `display-notation` rule (v17.1.0)
- `github` formatter removed (use `--custom-formatter @csstools/stylelint-formatter-github`)

Repos using Stylelint v16 may see minor differences in edge cases (selector counting inside `:is()`, `:has()`, `:where()`). These match the behavior they would get after upgrading to Stylelint v17.

Differential testing validates 0 FP / 0 FN against 22 real-world repos. Older Stylelint versions (v13, v14) may have behavioural differences that Gale does not replicate.

## Known gaps (bugs to fix, not acceptable limitations)

Every gap here is a bug. Gale must produce identical output to Stylelint v17 — "not yet supported" is not an acceptable answer.

1. **Missing rules** -- Any rule Stylelint has that Gale skips with "not yet supported" is a bug. Run the differential test to find them. They must be implemented.
2. **Arbitrary JavaScript plugins** -- Gale cannot execute JS plugins, but it has built-in Rust implementations of all standard plugin rules (@stylistic, scss, order) plus 4 declarative plugin meta-rules (`plugin/enforce-variable-for-property`, `plugin/no-unknown-custom-properties`, `plugin/no-unused-custom-properties`, `plugin/require-file-header-comment`) that cover the most common custom plugin patterns.
3. **Sass indented syntax** -- `.sass` files return `UnsupportedSyntax` error (`.scss` works fine).

## Intentional non-support

These are NOT bugs. They are deliberate scope decisions.

1. **`prettier/prettier` plugin** -- This Stylelint plugin runs the entire Prettier formatter and reports formatting diffs as lint warnings. Both the Stylelint and Prettier teams recommend running Prettier separately (`prettier --check`). The plugin pattern is in decline (~720K downloads/week but falling). Implementing it would require embedding a JS runtime or subprocess, negating Gale's speed advantage.
2. **Stylelint ≤15 behavioural quirks** -- Gale matches v17 behaviour only. Older versions have different vendor prefix handling, selector counting, and specificity calculations.
3. **Stylelint v16 compat mode** -- Gale does not provide a v16 compatibility mode. Projects using v16 may see minor differences in ~25 rules related to nesting, vendor prefixes, and specificity. These are the same changes they would encounter upgrading to Stylelint v17.
