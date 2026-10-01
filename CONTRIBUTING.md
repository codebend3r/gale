# Contributing to Gale

Thanks for your interest in contributing to Gale! This guide will help you get started.

## Building

```bash
cargo build
```

For a release build:

```bash
cargo build --release
```

## Testing

Run the full test suite:

```bash
cargo test --workspace
```

Run tests for a specific crate:

```bash
cargo test -p gale_linter
cargo test -p gale_config
```

Run a single test by name:

```bash
cargo test -p gale_linter block_no_empty
```

## Hooks

[Lefthook](https://lefthook.dev) runs the checks below automatically. It
installs itself when you run `bun install`; if the hooks ever go missing, run
`bunx lefthook install`.

| Hook | Checks | Warm cost |
| --- | --- | --- |
| `pre-commit` | `cargo fmt --check`, `cargo clippy -D warnings`, leftover conflict markers | under a second |
| `commit-msg` | subject is 72 characters or fewer, with no trailing period | instant |
| `pre-push` | `cargo test --workspace`, the bun feature suite against a debug build | around ten seconds |

Jobs are filtered by glob, so a docs-only commit skips the Rust checks
entirely. Release builds and the npm package matrix are left to CI, and the
benchmark runs weekly alongside the compatibility matrix.

To bypass a hook once:

```bash
git commit --no-verify
LEFTHOOK=0 git push
```

Personal overrides go in `lefthook-local.yml`, which is not tracked.

## Adding a new rule

Write the rule in `crates/gale_linter/src/rules/your_rule_name.rs`, implementing
the `Rule` trait, with its tests in a `#[cfg(test)] mod tests` block. Then list
it in `crates/gale_linter/src/rules/mod.rs`: declare `pub mod your_rule_name;`
and add one line to the `rules!` table at the end of the file, naming the
presets that enable it, if any:

```rust
  your_rule_name::YourRuleName [Recommended, GaleWarning],
```

That line is the only registration there is. It registers the rule, adds it to
`gale:all`, and puts it in each preset in brackets (see `Preset` in the same
file for the list). Tests fail if a rule file is missing from the table, or if
the rule counts in the README and docs go stale.

A few habits keep a rule from failing on real input:

- **Slice the source with `ctx.source_slice` / `ctx.source_from`**, which return `None` instead of panicking when an offset lands inside a multibyte character. Offsets built from parsed text (a re-serialised selector's length, say) do not always line up with what the author wrote; `ctx.selector_source` gives a style rule's selector as written.
- **Compile option patterns with `crate::pattern`**: `pattern::for_rule` for a `*-pattern` primary option (it returns the invalid-option report to hand back when the pattern does not compile), and `pattern::regex_entry` / `pattern::match_regex_entry` for `/regex/` entries in lists. Patterns are JavaScript regexes, lookaround included, and are compiled once and cached.
- **A panic is not the end of the run.** The runner catches it and reports an `Internal error` problem naming the rule, so one bad file does not hide the rest. This relies on panics unwinding: do not set `panic = "abort"` in a Cargo profile. `GALE_DEBUG_PANIC=<rule-name>` makes a rule panic on any file containing `gale-debug-panic`, to exercise the guard. `crates/gale_linter/tests/multibyte_no_panic.rs` lints sample files with multibyte characters inserted throughout, with every rule enabled.

## Differential testing

Differential tests compare Gale's output against Stylelint on real-world repositories to verify compatibility:

```bash
# Run against all repos
python tests/differential/run.py

# Run against a specific repo
python tests/differential/run.py bootstrap

# List available repos
python tests/differential/run.py --list

# Include timing comparison
python tests/differential/run.py --benchmark
```

See `tests/differential/` for more details.

## Pull requests

1. Fork the repo and create a branch from `main`
2. Make your changes
3. Make sure `bun run test` passes (build first with `cargo build --release`)
4. Make sure `bun run lint` is clean (clippy on every target, warnings denied)
5. Make sure `bun run fmt:check` passes
6. Open a PR with a clear description of what you changed and why

Steps 3 to 5 run on their own if you have the hooks installed (see
[Hooks](#hooks)).

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) runs these same
`package.json` scripts on every pull request and every push to `main`, plus the
npm package smoke tests on every supported Node version. Changes that touch
only Markdown, `docs/`, `.claude/`, or `LICENSE` skip CI.

## Releasing

Releases are tag-driven: see [PUBLISHING.md](PUBLISHING.md).

That's it. We try to keep the process lightweight.
