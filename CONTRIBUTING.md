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

## Adding a new rule

Gale has a well-defined process for adding lint rules. See [CLAUDE.md](CLAUDE.md) for the full walkthrough, but the short version is:

1. Create `crates/gale_linter/src/rules/your_rule_name.rs` implementing the `Rule` trait
2. Add `pub mod your_rule_name;` to `crates/gale_linter/src/rules/mod.rs`
3. Register the rule in `register_all()` in the same file
4. Add the rule name to `ALL_RULE_NAMES` in `crates/gale_config/src/lib.rs`
5. If appropriate, add it to `RECOMMENDED_ERROR_RULES` or `RECOMMENDED_WARNING_RULES`
6. Include tests in a `#[cfg(test)] mod tests` block inside the rule file

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
3. Make sure `cargo test --workspace` passes
4. Make sure `cargo clippy --workspace -- -D warnings` is clean
5. Make sure `cargo fmt --check` passes
6. Open a PR with a clear description of what you changed and why

That's it. We try to keep the process lightweight.
