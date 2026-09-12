# Gale vs. Stylelint feature table

A side-by-side comparison of what Gale and Stylelint each support. Gale targets
**Stylelint v17** semantics, so the Stylelint column describes v17 unless noted.

Legend:

| Symbol | Meaning |
|--------|---------|
| ✅ | Supported |
| ⚠️ | Partial support (see the description for the caveat) |
| ❌ | Not supported |

Gale's goal is byte-for-byte identical output to Stylelint, so most rows should
be ✅ in both columns. Rows where the two differ are the interesting ones: a ❌ or
⚠️ under Gale is either a known gap tracked as a bug or an intentional scope
decision; a ❌ under Stylelint is something Gale adds on top.

## Languages and syntaxes

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| CSS | Lint plain `.css` files | ✅ | ✅ |
| SCSS | Lint `.scss` files | ✅ Built in, no plugin needed | ✅ Via `postcss-scss` custom syntax |
| Less | Lint `.less` files | ✅ Built in, no plugin needed | ✅ Via `postcss-less` custom syntax |
| Sass indented syntax | Lint `.sass` files | ⚠️ Converted to SCSS internally; reported line/column drift from the original file | ✅ Via `postcss-sass` custom syntax |
| CSS-in-JS | Lint styles embedded in JS/TS (styled-components, etc.) | ❌ Files are skipped | ✅ Via `postcss-styled-syntax` and similar |
| HTML / Vue / Svelte | Lint `<style>` blocks in markup | ❌ Files are skipped | ✅ Via `postcss-html` |
| Markdown | Lint fenced CSS code blocks | ❌ Files are skipped | ✅ Via `postcss-markdown` |
| `customSyntax` config key | Choose a parser per file pattern | ⚠️ Accepts `postcss`, `postcss-scss`, `postcss-sass`, `postcss-less`; other values skip the matching files | ✅ Any PostCSS syntax package |
| CSS nesting | Parse and lint nested style rules | ✅ | ✅ |
| Parse-error recovery | Keep linting after a syntax error | ✅ | ✅ Reports `parseErrors` |

## Rules

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| Core Stylelint rules | The built-in rule set (`block-no-empty`, `property-no-unknown`, ...) | ✅ 144 rules built in | ✅ |
| `display-notation` (v17.1) | Newest core rule for `display` value notation | ✅ | ✅ |
| `@stylistic/*` rules | Formatting rules from `@stylistic/stylelint-plugin` | ✅ 69 rules built in | ✅ Via plugin |
| `scss/*` rules | Rules from `stylelint-scss` | ✅ 45 rules built in | ✅ Via plugin |
| `order/*` rules | Rules from `stylelint-order` | ✅ 3 rules built in | ✅ Via plugin |
| `plugin/*` declarative meta-rules | Design-token enforcement, custom-property analysis, file headers, browser compat | ✅ 5 rules | ❌ Requires a custom JS plugin |
| Vendor plugin rules | `csstools/value-no-unknown-custom-properties`, `material/no-prefixes`, `spectrum-tools/no-unknown-custom-properties` | ✅ 3 rules built in | ✅ Via the respective plugins |
| Arbitrary JavaScript plugins | Load any `stylelint-*` plugin from npm | ❌ Intentional: JS is not executed | ✅ |
| `prettier/prettier` | Report Prettier formatting diffs as lint warnings | ❌ Intentional: run Prettier separately | ✅ Via plugin |
| Rule severity per rule | `"error"` / `"warning"` and the `{ severity }` secondary option | ✅ | ✅ |
| `defaultSeverity` | Config-wide default when a rule does not set one | ✅ | ✅ |
| Custom `message` secondary option | Override the warning text for a rule | ✅ String form on every rule | ✅ String or function on every rule |
| `url` secondary option | Attach a docs URL to a rule's warnings | ✅ Emitted as the JSON warning's `url` | ✅ |
| `disableFix` secondary option | Keep a rule enabled but turn off its autofix | ✅ | ✅ |
| `reportDisables` secondary option | Forbid disabling a rule inline | ❌ | ✅ |
| Warning message wording | Exact v17 message text | ⚠️ Positions and rule IDs match; some strings still use v15/v16 phrasing | ✅ |
| Rule deprecation warnings | Report use of deprecated rules | ❌ Always empty in output | ✅ `deprecations` |
| Invalid option warnings | Report malformed rule options | ❌ Always empty in output | ✅ `invalidOptionWarnings` |

## Configuration

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| `.stylelintrc` (JSON / YAML) | Read the classic rc file | ✅ | ✅ |
| `.stylelintrc.json` / `.yml` / `.yaml` | Explicit-extension rc files | ✅ | ✅ |
| `stylelint.config.js` / `.mjs` / `.cjs` | JavaScript config files | ⚠️ Statically parsed, not executed; dynamic configs do not resolve | ✅ Executed |
| `.stylelintrc.js` / `.mjs` / `.cjs` | JavaScript rc files | ⚠️ Same static-parse caveat | ✅ Executed |
| `package.json` `"stylelint"` field | Inline config in the package manifest | ✅ | ✅ |
| `gale.json` / `gale.toml` | Gale's native config files | ✅ | ❌ |
| Config discovery | Walk up from the working directory to find a config | ✅ | ✅ |
| `--config` / `configFile` | Point at an explicit config file | ✅ | ✅ |
| `--config-basedir` | Base directory for resolving relative `extends` and plugins | ❌ | ✅ |
| `extends` shareable configs | Extend configs from npm packages in `node_modules/` | ✅ | ✅ |
| `extends` relative paths | Extend another config file by path | ✅ | ✅ |
| Recursive `extends` with cycle detection | Chains of shared configs | ✅ | ✅ |
| Built-in `stylelint-config-*` equivalents | `recommended`, `standard`, `recommended-scss`, `standard-scss` without installing them | ✅ | ❌ Must be installed from npm |
| `gale:recommended` / `gale:all` presets | Gale's own built-in presets | ✅ | ❌ |
| `rules` value formats | `true`, `false`, `null`, `"off"`, primary option, `[primary, { secondary }]` | ✅ Plus a severity-first array form | ✅ |
| `overrides` | Per-file-pattern rules, `extends`, and `customSyntax` | ✅ Including `ignoreFiles` inside an override | ✅ |
| `ignoreFiles` / `ignorePatterns` | Exclude files from the config | ✅ Both names accepted | ✅ `ignoreFiles` |
| `.stylelintignore` | Ignore file in gitignore syntax | ✅ | ✅ |
| `.galeignore` | Gale's own ignore file | ✅ | ❌ |
| `.gitignore` | Honor the repo's gitignore when discovering files | ❌ Intentional: only `.stylelintignore` and `.galeignore` apply, matching Stylelint | ❌ |
| `plugins` config key | Declare JS plugins to load | ⚠️ Accepted so configs parse; plugins are not executed | ✅ |
| `reportNeedlessDisables` config key | Enable needless-disable reporting from config | ✅ | ✅ |
| `ignoreDisables` config key | Ignore disable comments from config | ✅ | ✅ |
| `reportInvalidScopeDisables` config key | Enable from config | ✅ | ✅ |
| `reportDescriptionlessDisables` config key | Enable from config | ✅ | ✅ |
| `reportUnscopedDisables` config key | Report disable comments that name no rule | ✅ | ✅ |
| `allowEmptyInput` / `cache` / `cacheLocation` / `fix` / `quiet` config keys | Set CLI behaviour from config | ✅ A CLI flag still wins | ✅ |
| `languageOptions` | Extend known at-rules, properties, types, and CSS-wide keywords | ❌ | ✅ |
| `computeEditInfo` | Include fix edit ranges in warnings | ❌ | ✅ |
| `validate` | Toggle rule-option validation | ❌ | ✅ |
| `--print-config` | Print the resolved config as JSON | ✅ | ✅ |
| Starter config generator | Scaffold a config in the current directory | ✅ `--init` | ⚠️ Via the separate `npm init stylelint` package |

## Inline disable comments

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| `stylelint-disable` / `stylelint-enable` | Disable rules over a range | ✅ | ✅ |
| `stylelint-disable-line` | Disable rules on the current line | ✅ | ✅ |
| `stylelint-disable-next-line` | Disable rules on the following line | ✅ | ✅ |
| Rule lists in disable comments | `stylelint-disable rule-a, rule-b` | ✅ | ✅ |
| `-- description` suffix | Explain why a rule is disabled | ✅ | ✅ |
| `gale-*` comment prefix | Gale's own alias for every disable form | ✅ | ❌ |
| `--ignore-disables` | Ignore every disable comment | ✅ | ✅ |
| `--report-needless-disables` | Report comments that suppress nothing | ✅ | ✅ |
| `--report-invalid-scope-disables` | Report comments for rules not being linted | ✅ | ✅ |
| `--report-descriptionless-disables` | Report comments with no description | ✅ | ✅ |
| `--report-unscoped-disables` | Report comments that disable all rules | ✅ | ✅ |

## Command line

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| Files and glob patterns | Positional file, directory, or glob arguments | ✅ | ✅ |
| `--globby-options` | Tune glob expansion | ❌ | ✅ |
| `--ignore-path` | Custom ignore file | ✅ | ✅ |
| `--ignore-pattern` / `--ip` | Extra ignore globs on the command line | ✅ | ✅ |
| `--disable-default-ignores` / `--di` | Lint `node_modules` too | ✅ | ✅ |
| `--no-ignore` | Skip every ignore file | ✅ | ❌ |
| `--stdin` / `--stdin-filename` | Lint source from standard input | ✅ | ✅ |
| `--fix` | Autofix problems (strict mode by default) | ✅ | ✅ |
| `--fix=lax` | Also fix files that have parse errors | ✅ | ✅ |
| `--formatter` / `-f` | Pick an output format | ✅ | ✅ |
| `--custom-formatter` | Load a formatter from a JS module | ❌ | ✅ |
| `--custom-syntax` | Pick a parser from the command line | ⚠️ Same four syntaxes as the config key; anything else skips every file | ✅ Any PostCSS syntax package |
| `--quiet` / `-q` | Only report errors | ✅ | ✅ |
| `--quiet-deprecation-warnings` | Silence deprecation notices | ✅ Accepted; Gale has none to silence | ✅ |
| `--max-warnings` | Fail when warnings exceed a threshold | ✅ | ✅ |
| `--cache` | Skip unchanged files on repeat runs | ✅ | ✅ |
| `--cache-location` | Where the cache file lives | ✅ | ✅ |
| `--cache-strategy` | Choose `metadata` or `content` invalidation | ❌ Content hashing only | ✅ |
| `--allow-empty-input` | Exit 0 when no files match | ✅ | ✅ |
| `--output-file` / `-o` | Write the report to a file, colour stripped | ✅ | ✅ |
| `--color` / `--no-color` | Force or suppress ANSI colour | ✅ Same rule as picocolors: `NO_COLOR`, `FORCE_COLOR`, `CI`, TTY | ✅ |
| `--print-config` | Print the resolved config for a file | ✅ | ✅ |
| `--validate` / `--no-validate` | Toggle option validation | ❌ | ✅ |
| `--init` | Generate a starter config | ✅ | ❌ |
| `--lsp` | Start the language server | ✅ | ❌ |
| `--version` / `--help` | Standard CLI help | ✅ | ✅ |
| Exit code `2` on lint problems | Error-severity problems or an exceeded `--max-warnings` | ✅ | ✅ |
| Exit code `1` when no files match | Empty input is a fatal error unless allowed | ✅ | ✅ |
| Exit code `64` for usage errors | Unknown flag, unknown formatter | ✅ | ✅ |
| Exit code `78` for config errors | A config file that cannot be loaded | ✅ | ✅ Also invalid rule options |

## Output formatters

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| `string` (text) | Human-readable default | ✅ `text` is the default; `string` is accepted as an alias | ✅ |
| `json` | Stylelint-shaped JSON array of `{ source, warnings }` | ✅ Field-for-field match, including `endLine` / `endColumn` | ✅ |
| `compact` | One problem per line | ✅ | ✅ |
| `verbose` | Summary with per-rule counts | ✅ | ✅ |
| `tap` | Test Anything Protocol | ✅ | ✅ |
| `unix` | `file:line:col: message [rule]` | ✅ | ✅ |
| `github` | GitHub Actions annotations | ❌ Removed in v17 | ❌ Removed in v17 |
| Custom formatter module | Any exported function | ❌ | ✅ `--custom-formatter` |
| Unknown formatter rejected | Typo in `--formatter` is an error, not a silent fallback | ✅ | ✅ |

## Autofix

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| Fix on the command line | `--fix` rewrites files in place | ✅ | ✅ |
| Strict and lax modes | Skip or include files with parse errors | ✅ | ✅ |
| Fixed output on stdin | `--fix --stdin` prints the fixed source | ✅ | ✅ |
| Repeated passes until stable | Re-run fixes when one fix enables another | ✅ | ⚠️ Single pass |
| Disable comments suppress fixes | A disabled rule does not fix its range | ✅ | ✅ |
| `disableFix` per rule | Report but never fix a specific rule | ❌ | ✅ |
| Edit info in results | Fix ranges returned without writing files | ❌ | ✅ `computeEditInfo` |

## Programmatic API (Node.js)

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| `lint()` | Lint files or a code string and get a `LinterResult` | ✅ Shells out to the `gale` binary | ✅ |
| `lint({ code, codeFilename })` | Lint an in-memory string | ✅ | ✅ |
| `lint({ config })` inline config | Pass a config object instead of a file | ✅ | ✅ |
| `lint({ fix })` | Autofix from the API | ✅ | ✅ |
| `result.results` | Per-file results with `warnings` | ✅ | ✅ |
| `result.errored` / `result.report` | Aggregate status and formatted output | ✅ | ✅ |
| `result.ruleMetadata` | Metadata for every rule that ran | ❌ Always empty | ✅ |
| `deprecations` / `invalidOptionWarnings` / `parseErrors` | Per-file diagnostic arrays | ❌ Present but always empty | ✅ |
| `resolveConfig()` | Resolve the effective config for a file | ✅ | ✅ |
| `formatters` | Promise-based formatter functions | ✅ Built-in six | ✅ |
| `createPlugin()` | Author a rule in JavaScript | ❌ No-op stub that warns | ✅ |
| `utils` (`report`, `ruleMessages`, `validateOptions`, `checkAgainstRule`) | Helpers for plugin authors | ❌ | ✅ |
| `rules` | Access built-in rule implementations | ❌ | ✅ |
| ESM and CommonJS entry points | `import` and `require` both work | ✅ | ✅ |
| TypeScript types | Type declarations shipped with the package | ✅ `index.d.ts` | ✅ |

## Editor integration

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| Built-in language server | LSP diagnostics from the linter binary itself | ✅ `gale --lsp` | ❌ Third-party wrappers only |
| VS Code extension | Diagnostics in VS Code | ⚠️ Separate repo, [codebend3r/gale-plugin](https://github.com/codebend3r/gale-plugin); not on the Marketplace | ✅ Official `stylelint.vscode-stylelint` |
| Neovim / Helix / Zed | Any LSP-capable editor | ✅ Point the editor at `gale --lsp` | ⚠️ Via community language servers |
| Lint on change and on save | Diagnostics refresh as you type | ✅ | ✅ |
| Quick-fix code actions | Apply autofixes from the editor | ❌ | ✅ In the official extension |

## Distribution and platforms

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| npm package | `npm install -D ...` | ✅ `@codebend3r/gale` with prebuilt binaries | ✅ `stylelint` |
| Cargo | `cargo install gale-lint` | ✅ | ❌ |
| Prebuilt binaries on GitHub Releases | Download and run without a package manager | ✅ | ❌ |
| macOS (arm64, x64) | Runs natively | ✅ | ✅ |
| Linux (x64, arm64) | Runs natively | ✅ | ✅ |
| Windows | Runs natively | ⚠️ Build from source with Cargo; no prebuilt binary | ✅ |
| No Node.js runtime required | Run without Node installed | ✅ | ❌ |
| Zero-dependency install | No `postinstall` script, no downloads at install time | ✅ | ❌ Pulls the PostCSS dependency tree |

## Performance

| Feature | Description | Gale | Stylelint |
|---------|-------------|:----:|:---------:|
| Parallel linting | Use every CPU core | ✅ | ❌ Single-threaded |
| Fast file discovery | Ripgrep-style directory walking with ignore support | ✅ | ⚠️ Globby |
| Per-phase timing output | `GALE_DEBUG_PERF=1` prints phase timings | ✅ | ❌ |
| Structured logging | `GALE_LOG=debug` tracing output | ✅ | ❌ |
| Differential test harness | Compare output against Stylelint on real repos | ✅ 22-repo corpus | n/a |
