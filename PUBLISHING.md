# Publishing Gale to npm

Gale is distributed on npm as `@codebend3r/gale`. The package includes
precompiled binaries for supported platforms in the npm tarball, so install does
not run a lifecycle script or download executables from GitHub Releases.

## Package layout

```
npm/
  package.json    @codebend3r/gale — main package
  bin/gale        POSIX launcher that selects the current platform binary
  bin/<target>/   Precompiled platform binaries
  README.md       npm page README
```

When a user runs `npm install @codebend3r/gale`, package managers unpack the
launcher and the platform binaries directly from the npm tarball. Running
`gale` executes `bin/gale`, which selects the matching native binary.

Supported platforms: `darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64`.

## Prerequisites

- npm account with publish access to the `@codebend3r` scope
- Precompiled binaries for every supported platform

## Release process

### Option A: Automated (GitHub Actions)

Push a git tag matching `v*` (e.g. `v0.2.1`) to trigger the release workflow:

```bash
# 1. Update workspace.package.version in Cargo.toml
# 2. Commit the version bump
git tag v0.2.1
git push && git push --tags
```

The workflow will:
1. Build Linux binaries (x64 + arm64)
2. Build macOS binaries (x64 + arm64)
3. Create a GitHub Release with the binaries
4. Stage the binaries into `npm/bin/<target>/gale`
5. Set the npm package version to match
6. Publish `@codebend3r/gale` to npm

### Option B: Manual

```bash
# 1. Build and stage the binary for your platform
./scripts/build-npm.sh

# 2. Set the version
./scripts/build-npm.sh --version 0.2.1

# 3. Publish
cd npm && npm publish --access public
```

Note: Production npm releases must include binaries for all supported platforms.
Use the CI release workflow for production releases.

## Rust targets reference

| Platform | Rust target | GitHub Release artifact name |
|---|---|---|
| macOS ARM (M1+) | `aarch64-apple-darwin` | `gale-aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` | `gale-x86_64-apple-darwin` |
| Linux x64 | `x86_64-unknown-linux-gnu` | `gale-x86_64-unknown-linux-gnu` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | `gale-aarch64-unknown-linux-gnu` |

## Crates.io

The Rust crate is published separately as `gale-lint` on crates.io (since the
name `gale` was already taken). The binary it installs is still called `gale`.

```bash
cargo install gale-lint
```

## Testing locally

```bash
# 1. Build and stage for your current platform
./scripts/build-npm.sh

# 2. Pack (dry run) to see what would be published
cd npm && npm pack --dry-run

# 3. Test the full install flow locally
mkdir /tmp/gale-test && cd /tmp/gale-test
npm init -y
npm install /path/to/gale/npm
npx gale --version
```

## Troubleshooting

**"Gale binary missing"**: The npm tarball did not include the expected
`bin/<target>/gale` file. Reinstall `@codebend3r/gale`; if the error
persists, report a packaging bug.

**Unsupported platform**: The npm launcher supports darwin and linux on
arm64/x64. Windows is not yet supported via npm. Build from source instead.
