# Publishing Gale

One `v*` tag publishes Gale everywhere it ships:

- **npm**: `@codebend3r/gale`, with a prebuilt binary for every supported platform inside the tarball
- **crates.io**: `gale-lint` (the `gale` binary) and the seven `gale_*` library crates it depends on
- **GitHub Releases**: the raw binaries plus a `SHA256SUMS` file

The [release workflow](.github/workflows/release.yml) does all three. Until its
one-time setup is done, publish from your machine instead (see
[Fallback: publish locally](#fallback-publish-locally)).

## Package layout

```
npm/
  package.json    @codebend3r/gale
  bin/gale.cjs    Node launcher that runs the binary for the current platform
  bin/<target>/   Prebuilt binary per platform: gale, or gale.exe on Windows
  platform.cjs    Maps platform-arch to a Rust target; the release builds exactly these
  index.mjs       Programmatic API (index.cjs for require, index.d.ts for types)
  README.md       npm page README
```

Installing `@codebend3r/gale` unpacks the launcher and every platform binary
straight from the tarball: no lifecycle script, no download from GitHub
Releases. Running `gale` executes `bin/gale.cjs`, which picks the binary for the
current platform.

Supported platforms: `darwin-arm64`, `darwin-x64`, `linux-arm64`, `linux-x64`,
`win32-arm64`, `win32-x64`. Releases published by hand, before this workflow
worked, ship without the Windows binaries.

## Releasing

1. **Bump.** Move every version reference to `X.Y.Z` in one commit named
   `X.Y.Z`: `workspace.package.version` and the seven internal crate pins in
   `Cargo.toml`, `Cargo.lock` (`cargo update --workspace`), `package.json`,
   `npm/package.json`, and the version examples in the docs. The
   `npm-version-release` skill does this and sweeps for stragglers.
2. **Dry run (optional, recommended).** Push the commit, then run the workflow
   by hand with dry run ticked:

   ```bash
   gh workflow run release.yml --ref main -f dry-run=true
   gh run watch "$(gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId')"
   ```

3. **Tag and push.**

   ```bash
   git tag -a vX.Y.Z -m X.Y.Z
   git push origin main vX.Y.Z
   ```

4. **CI publishes.** The workflow runs:
   1. **Version guard**: fails at once, listing every mismatch, unless the tag
      equals `workspace.package.version`, every internal crate pin, the gale
      entries in `Cargo.lock`, `package.json`, and `npm/package.json`.
   2. **Build** each target in the table below on its own runner, checking
      that every binary the runner can execute reports `gale X.Y.Z`.
   3. **npm**: stage a binary for every platform `npm/platform.cjs` maps
      (refusing to publish if one is missing), check the file list, and
      publish with provenance.
   4. **crates.io**: one `cargo publish` for every crate not yet at `X.Y.Z`.
      Cargo orders them by dependency, verifies each one builds from its
      packaged sources, and waits for each to reach the index before
      publishing the crates that depend on it.
   5. **GitHub Release**: created last, so a release on GitHub always means
      npm and crates.io have it too. Notes are generated from the commits.

### Re-running a failed release

Every publish step skips whatever is already published at that version, so
fix the cause (a missing secret, a registry outage) and re-run:

```bash
gh run rerun <run-id> --failed
```

If the version guard failed, the tag points at a commit with the wrong
versions. Fix the manifests, amend the release commit, and move the tag:

```bash
git tag -fa vX.Y.Z -m X.Y.Z
git push --force-with-lease origin main
git push -f origin vX.Y.Z
```

### What a dry run does

A dispatch with **dry run** ticked runs the whole pipeline, publishing nothing:
the version guard (checked against `Cargo.toml` when not run from a tag), all
six builds, the npm staging and file list, `npm publish --dry-run`, and
`cargo publish --dry-run`, which packages and verifies every crate. It skips
the GitHub Release.

A dry run skips versions that are already published, exactly as a real run
would (npm refuses even a dry run of a version it already has). To exercise
the publish steps, run it after the bump and before the tag.

A dispatch with dry run unticked is refused unless it runs from a `v*` tag.

A dry run dispatched from `main` also saves the build caches, so the tag run
that follows starts warm. Caches saved by tag runs are not reusable.

## One-time setup

### 1. Enable the workflow

Every run before this setup failed at the npm publish step, so the workflow was
disabled. Turn it back on once npm and crates.io are configured:

```bash
gh workflow enable release.yml
```

### 2. npm: trusted publishing

Trusted publishing lets the workflow publish with a short-lived OIDC token
instead of a stored secret, and attaches provenance automatically.

On [npmjs.com](https://www.npmjs.com/package/@codebend3r/gale), open
**Settings** for `@codebend3r/gale`, then **Trusted Publisher**, choose
**GitHub Actions**, and enter:

| Field | Value |
|---|---|
| Organization or user | `codebend3r` |
| Repository | `gale` |
| Workflow filename | `release.yml` |
| Environment name | leave empty |

After the first release succeeds this way, you can lock the package down under
**Settings > Publishing access > Require two-factor authentication and
disallow tokens**.

**Fallback: an `NPM_TOKEN` secret.** If you would rather not use trusted
publishing, create a granular access token on npmjs.com with read and write
access to `@codebend3r/gale`, allowed to bypass two-factor authentication, and
store it:

```bash
gh secret set NPM_TOKEN
```

npm tries OIDC first and falls back to the token. Write tokens expire (90 days
at most), which is why trusted publishing is the better default.

### 3. crates.io: `CARGO_REGISTRY_TOKEN`

On [crates.io/settings/tokens](https://crates.io/settings/tokens), create a
token from the account that owns the crates (`cargo owner --list gale-lint`
shows who that is) with:

- **Scopes:** `publish-update` (add `publish-new` only when adding a new crate)
- **Crates:** `gale-lint` and `gale_*`

Then store it:

```bash
gh secret set CARGO_REGISTRY_TOKEN
```

## Fallback: publish locally

Until the setup above is done, publish from your machine. This path cannot build
the Windows binaries, so a release published this way supports macOS and Linux
only.

```bash
# 1. Build and stage all four macOS and Linux binaries (Linux needs cross and Docker)
./scripts/build-npm.sh --all
node ./npm/bin/gale.cjs --version     # must print the new version

# 2. Check the tarball, then publish (npm 11 opens the browser for 2FA)
cd npm
npm pack --dry-run
npm publish --access public

# 3. Optionally, the crates (needs `cargo login` first)
cd ..
cargo publish --workspace --locked
```

## Rust targets reference

| Platform | Rust target | Built on | GitHub Release asset |
|---|---|---|---|
| macOS ARM (M1+) | `aarch64-apple-darwin` | `macos-latest` | `gale-aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` | `macos-latest` (cross) | `gale-x86_64-apple-darwin` |
| Linux x64 | `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | `gale-x86_64-unknown-linux-gnu` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | `gale-aarch64-unknown-linux-gnu` |
| Windows x64 | `x86_64-pc-windows-msvc` | `windows-latest` | `gale-x86_64-pc-windows-msvc.exe` |
| Windows ARM64 | `aarch64-pc-windows-msvc` | `windows-latest` (cross) | `gale-aarch64-pc-windows-msvc.exe` |

Adding a platform means adding it to `npm/platform.cjs` and to the build matrix
together. `tests/features/npm-package.test.ts` fails if the two disagree, and
the npm job refuses to publish a package with a missing binary.

## Crates.io

The binary crate is `gale-lint` on crates.io (the name `gale` was taken). The
binary it installs is still called `gale`.

```bash
cargo install gale-lint
```

## Testing locally

```bash
# 1. Build and stage the binary for your platform
./scripts/build-npm.sh

# 2. See what would be published
cd npm && npm pack --dry-run

# 3. Install the package into a scratch project
mkdir /tmp/gale-test && cd /tmp/gale-test
bun init -y
bun add /path/to/gale/npm
bunx gale --version
```

## Troubleshooting

**"Gale binary missing"**: the tarball did not include `bin/<target>/gale` (or
`gale.exe`) for this platform. Reinstall `@codebend3r/gale`; if the error
persists, report a packaging bug.

**Unsupported platform**: the launcher covers the six platforms listed above.
Anywhere else, build from source or use `cargo install gale-lint`.

**Version guard failed**: see [Re-running a failed
release](#re-running-a-failed-release).

**npm publish failed with `ENEEDAUTH`, `E401`, or `E404`**: npm had no
credentials it could use. Check that the trusted publisher fields match the
table above exactly (the workflow filename is `release.yml`, not a path), or
set `NPM_TOKEN`, then re-run the failed jobs.

**crates.io publish failed**: a missing `CARGO_REGISTRY_TOKEN` fails with a
message saying so. Anything else, such as a crate that does not build from its
packaged sources, shows up in a dry run first.
