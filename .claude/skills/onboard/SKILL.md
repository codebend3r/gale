---
name: onboard
description: Use when setting up gale in this repo (path contains `git/gale`) on a new machine or a fresh clone, or when any `package.json` script fails for environment reasons rather than code reasons: "cargo: command not found", "'cross' is not installed", "hyperfine not found", "lefthook: not found", `bun build:all` / `bun run lint` / `bun test:rust` exiting before it compiles anything, or the user asks whether everything needed to run the scripts is installed.
---

# Onboard a machine onto gale

## Overview

Every script in `package.json` leans on a tool outside the repo: `cargo` and its
rustup components, `cross` plus a running Docker daemon, `bun`, `node`, `hyperfine`,
`python3`. This skill checks all of them, applies the fix for whatever is missing,
then proves the scripts actually run.

**The failure this skill exists for:** rustup installs `cargo`, `rustfmt`, `clippy`
and `cross` as shims in `~/.cargo/bin`, which is on `PATH` only if the shell sources
`~/.cargo/env`. When it doesn't, eight scripts fail with messages that blame the wrong
thing. `build:all` reports `ERROR: 'cross' is not installed` while `cross` sits
installed on disk. **Never take those messages at face value: check `~/.cargo/bin`
before installing anything.**

## 1. Run the doctor

```bash
bash .claude/skills/onboard/check.sh
```

Read-only. It prints `ok` / `FAIL` / `warn` per check and names each failure with an
id. `FAIL` blocks a script outright; `warn` blocks only `build:all` or the benchmarks.
Exit status is 0 only when every required check passed.

## 2. Apply the fix for each reported id

Work through every id the doctor printed. Do not stop at the first one.

| id | Fix |
|---|---|
| `cargo-path` | Rustup is installed but invisible. Add it to the user's shell rc (see the block below), then `export PATH="$HOME/.cargo/bin:$PATH"` for the shell you are in. |
| `cargo` | Genuinely absent: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| `component-rustfmt` | `rustup component add rustfmt` |
| `component-clippy` | `rustup component add clippy` |
| `target-<triple>` | `rustup target add <triple>` |
| `node`, `node-version` | Install Node >= 20 (`package.json` `engines`). This repo's user runs fnm: `fnm install --lts && fnm use --lts` |
| `bun` | `curl -fsSL https://bun.sh/install \| bash` |
| `node_modules` | `bun install` |
| `cross` | `cargo install cross` |
| `docker`, `docker-daemon` | Install and start Docker Desktop. Only `build:all`'s two Linux targets need it. |
| `hyperfine` | `brew install hyperfine` |
| `python3` | `brew install python@3.14` |
| `release-binary` | `bun run build:release` |

The `cargo-path` rc edit, guarded so a re-run cannot append a second copy:

```bash
grep -q 'cargo/env' ~/.zshrc || cat >> ~/.zshrc <<'EOF'

# Rust toolchain (rustup shims: cargo, rustc, clippy, rustfmt, cross)
[ -s "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
EOF
```

Verify it took in a fresh login shell, not the current one:

```bash
zsh -lic 'command -v cargo; command -v cross'
```

Re-run the doctor after fixing. It must exit 0 before moving on.

## 3. Prove the scripts run

A green doctor is not proof. Run the real thing, in this order, and report actual output:

```bash
bun run fmt:check      # cargo fmt --all --check
bun run lint           # cargo clippy --workspace --all-targets -- -D warnings
bun run build:release  # produces target/release/gale, which test:api needs
bun run test           # test:rust, then test:api, then test:features
```

`bun run system-check` chains exactly these. Use it once the individual steps pass;
running it first makes a failure harder to attribute.

Only if the machine needs to cut a release:

```bash
bun run build:all      # 4 targets into npm/bin/, needs cross + Docker
```

Expect roughly 15s per macOS target and 35s per Linux target on Apple Silicon, plus a
one-time Docker image pull. Verify the output rather than trusting the exit code:

```bash
for f in npm/bin/*/gale; do printf "%-34s " "${f#npm/bin/}"; file -b "$f" | cut -c1-40; done
```

Four binaries: two Mach-O (arm64, x86_64), two ELF (aarch64, x86-64).

## Things that look broken but are not

- **`bun build:all` is valid.** `bun <script>` runs a `package.json` script; it does
  not collide with bun's own `bun build` bundler here. If it fails, the script inside
  failed, so read past bun's `error: script "build:all" exited with code 1` to the
  real message above it.
- **A shell opened before the `~/.zshrc` fix keeps the old `PATH`.** The fix applies to
  new shells. Tell the user to open a new terminal or `source ~/.zshrc`; do not
  conclude the fix failed. `scripts/build-npm.sh` prepends `~/.cargo/bin` itself, so it
  works either way, but `bun run lint` and the other `cargo` scripts do not.
- **`cross` warns it has no image for `aarch64-apple-darwin`.** Expected, and harmless.
  `scripts/build-npm.sh` hands `cross` only the two Linux targets and builds both macOS
  arches with plain `cargo`, because `cross` has no macOS images at all. If you ever
  rework that script, keep the split: routing a Darwin target through `cross` fails.

## Red flags

- About to `cargo install cross` or `brew install rust` before checking `~/.cargo/bin`.
- Reporting the machine as ready with only the doctor's output as evidence.
- Fixing the first `FAIL` and re-running one script instead of clearing every id.
- Editing `~/.zshrc` without grepping for an existing `.cargo/env` line first.
