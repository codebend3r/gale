---
name: npm-version-release
description: Use when the user asks for a release of gale in this repo (path contains `git/gale`) — "npm version patch", "npm version minor", "npm version 0.3.0", "release 0.3.0", "publish 0.3.0", "bump to 0.3.0", "cut 0.3.0", "publish to npm", or re-publishing a version that already has a tag. Applies to any version bump in this repo, whether given as a semver or as patch/minor/major.
argument-hint: <semver | patch | minor | major>
---

# Release Gale

## Overview

A release is **one commit named `X.Y.Z` on the current branch** that carries every version reference in the repo, and an annotated tag `vX.Y.Z` on that commit. Pushing the tag runs `.github/workflows/release.yml`, which builds all six platform binaries, publishes `@codebend3r/gale` to npm and the `gale-lint` crates to crates.io, and creates the GitHub Release. The whole thing, through confirming the registries have the new version, is this skill's job.

**No release branch. No pull request. No `git reset --hard`.** The user commits releases directly on the branch they are on. Do not branch or switch branches. Do not ask which of several release strategies to use — this file is the strategy.

There are two paths. Use the **CI path** whenever the release workflow is set up (step 1 checks). Use the **local fallback** only when it is not, and say so in the first message, because the local path cannot build the Windows binaries.

`PKG` is the `name` in `npm/package.json` (read it, do not hardcode it).

## Resolve VERSION

`VERSION` is a bare semver, no `v`, and plain `MAJOR.MINOR.PATCH` (the workflow's version guard rejects prereleases).

- User gave a semver ("npm version 0.3.0", "release 0.3.0"): that is `VERSION`.
- User gave a bump word ("npm version patch", "bump minor", "npm version update"): compute it from what is **published**, not from a manifest, because a half-finished earlier release can leave the manifests ahead of the registry.

```bash
PKG=$(bun -p "require('./npm/package.json').name")
CUR=$(bun info "$PKG" version)          # published version, the real baseline
VERSION=$(bun -e "const [M,m,p]='$CUR'.split('.').map(Number);const k=process.argv[1];console.log(k==='major'?\`\${M+1}.0.0\`:k==='minor'?\`\${M}.\${m+1}.0\`:\`\${M}.\${m}.\${p+1}\`)" patch)   # patch | minor | major
echo "$CUR -> $VERSION"
```

"update" with no qualifier means `patch`. State the computed `VERSION` in the first message to the user and keep going; do not wait for confirmation.

## Procedure

### 1. Preflight (read-only)

```bash
git status --short                     # must be empty
git rev-parse --abbrev-ref HEAD        # note it; stay on it
git fetch origin --tags --force
git rev-list --left-right --count HEAD...@{u}   # "<ahead> <behind>"
git tag -l "v$VERSION"                 # non-empty means a re-release: see step 9
gh api "repos/{owner}/{repo}/actions/workflows/release.yml" --jq .state   # "active" means CI is set up
gh secret list --json name --jq '.[].name'                                # must include CARGO_REGISTRY_TOKEN
```

**CI path** when the workflow state is `active` and `CARGO_REGISTRY_TOKEN` is listed. npm auth is either a trusted publisher on npmjs.com (not visible from here) or an `NPM_TOKEN` secret; the owner enables the workflow only after configuring it (see `PUBLISHING.md`). Otherwise use the **local fallback** (step 10) and tell the user which piece of setup is missing.

`behind` must be 0 for a fresh release. `behind` greater than 0 is acceptable only when the remote-only commits are the earlier attempt at this same `VERSION` (step 9). Any other divergence: stop and report it; do not reset or rebase on your own.

### 2. Bump the manifests

`Cargo.toml` pins the workspace version and the seven internal crate versions to the same string, so a single substitution covers all of them. The Cargo version and the npm version can differ if a previous release skipped one, so capture both.

```bash
OLD_CARGO=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OLD_NPM=$(bun -p "require('./npm/package.json').version")
sed -i '' "s/version = \"$OLD_CARGO\"/version = \"$VERSION\"/g" Cargo.toml
grep -c "version = \"$VERSION\"" Cargo.toml   # must be 8: workspace + 7 internal crates; anything else means a third-party pin matched, so inspect `git diff Cargo.toml`
cargo update --workspace               # rewrites the gale_* entries in Cargo.lock
bun pm pkg set version="$VERSION"
(cd npm && bun pm pkg set version="$VERSION")
```

Skip the Cargo substitution only if `OLD_CARGO` already equals `VERSION`. Without it the binaries print the old version, and the workflow's version guard rejects the tag.

### 3. Sweep every other version reference

The manifests are not the only place the version lives. These files carry it too and must move in the same commit:

| File | What it holds |
|---|---|
| `README.md` | `build-npm.sh --version X.Y.Z` example |
| `scripts/build-npm.sh` | usage comment `--version X.Y.Z` |
| `tests/differential/open_prs.py` | `GALE_VERSION = "X.Y.Z"`, written into migration PRs |
| `docs/index.html` | `"@codebend3r/gale": "^X.Y.Z"` in the install snippet |

Rewrite the known files, then run the sweep. The sweep is the real check; the table is only what was true last time.

```bash
for f in README.md scripts/build-npm.sh tests/differential/open_prs.py docs/index.html; do
  perl -pi -e "s/\b(v?)(\Q$OLD_NPM\E|\Q$OLD_CARGO\E)\b/\${1}$VERSION/g" "$f"
done
# perl, not sed: BSD sed has no \b and silently matches nothing. \${1} not \$1: "$1" + "0.2.2" is read as group $10.
# Sweep: any 0.x.y in a tracked file that is not VERSION. Must print nothing.
git ls-files \
  | grep -vE '(^|/)(Cargo\.lock|bun\.lock|package-lock\.json)$|^MIGRATION_PRS\.md$|^benchmarks/|^npm/bin/|^\.claude/skills/' \
  | xargs grep -nE '\bv?0\.[0-9]+\.[0-9]+\b' 2>/dev/null | grep -vF "$VERSION"
git diff --stat
```

Any line the sweep prints is either a gale version that still needs updating (fix it, add the file to the table above) or a third-party version that happens to start with `0.` (leave it, add its path to the exclusion list). Do not proceed with hits you have not classified.

Excluded on purpose: `Cargo.lock` and the JS lockfiles (third-party pins; the gale crates are handled by `cargo update`), `MIGRATION_PRS.md` (records which version each upstream PR was opened against; history, not a reference), `benchmarks/` (cloned repos), `npm/bin/` (binaries), `.claude/skills/` (illustrative semvers in skill docs, not references).

### 4. Commit and tag on the current branch

```bash
git diff --stat                        # every file from steps 2 and 3, nothing else
git add -A
git commit -m "$VERSION"
git tag -a "v$VERSION" -m "$VERSION"
```

On the CI path the commit carries no binaries: CI builds every platform from the tag. The `npm/bin/<target>/gale` files tracked in git are not what npm users get, so leave them alone.

### 5. Push the commit and dry-run the release

Push the commit, not the tag yet, then run the whole pipeline without publishing. The dry run catches anything that would fail after the tag is public: a version the guard rejects, a target that does not build, a crate that does not build from its packaged sources.

```bash
git push origin HEAD
gh workflow run release.yml --ref "$(git rev-parse --abbrev-ref HEAD)" -f dry-run=true
sleep 5
RUN=$(gh run list --workflow release.yml --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId')
gh run watch "$RUN" --exit-status
```

If it fails, fix the cause, amend the release commit, move the local tag (`git tag -fa "v$VERSION" -m "$VERSION"`), push with `--force-with-lease`, and dry-run again.

### 6. Push the tag

```bash
git push origin "v$VERSION"
sleep 5
RUN=$(gh run list --workflow release.yml --event push --limit 1 --json databaseId --jq '.[0].databaseId')
gh run watch "$RUN" --exit-status      # builds take several minutes; use the 10-minute timeout and re-attach if needed
```

### 7. If the tag run fails

Every publish job skips what is already published, so a failed run is re-run as is once the cause is fixed:

```bash
gh run view "$RUN" --log-failed | tail -40
gh run rerun "$RUN" --failed
```

A failure the user has to fix (a missing or expired `NPM_TOKEN` or `CARGO_REGISTRY_TOKEN`, a trusted publisher that does not match `codebend3r` / `gale` / `release.yml`) is the one thing to hand back. Say exactly which setting, then re-run once they confirm.

If the version guard failed, the tag points at the wrong commit: fix the manifests, amend, and move the tag with `git tag -fa` and `git push -f origin "v$VERSION"`.

### 8. Verify

The registries take a few minutes to show a new version, so poll instead of checking once:

```bash
for i in $(seq 1 30); do
  [ "$(bun info "$PKG" version 2>/dev/null)" = "$VERSION" ] && break; sleep 10
done
bun info "$PKG" version                 # must equal VERSION
curl -s https://index.crates.io/ga/le/gale-lint | tail -1 | grep -o '"vers":"[^"]*"'   # must show VERSION
gh release view "v$VERSION" --json assets --jq '.assets[].name'   # six binaries and SHA256SUMS
```

Report the tag commit, the run URL, and the npm, crates.io, and GitHub Release versions in the final message.

### 9. Re-releasing a version that already exists

If step 1 found the tag already exists locally or on origin (they may point at different commits), the previous attempt stopped partway.

- **The tag run failed partway and the commit is right:** re-run it (step 7). Nothing else.
- **The commit is wrong:** redo steps 2 to 6 with these differences: `git commit --amend` onto the existing `X.Y.Z` commit instead of a new commit; every tag write is `git tag -f`, every tag push is `git push -f`; and if origin's branch already has the old commit, push the branch with `git push --force-with-lease origin HEAD`.

### 10. Local fallback

Only when step 1 found the release workflow disabled or unconfigured. This path builds the four macOS and Linux binaries locally and publishes npm from this machine; it cannot build Windows binaries, so call that out to the user.

After steps 2 to 4, build all four targets into `npm/bin/<triple>/gale`. `scripts/build-npm.sh` builds the host target with `cargo build --release` and every other target with `cross build --release --target <triple>`.

```bash
command -v cargo || { curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; . "$HOME/.cargo/env"; }
command -v cross || cargo install cross
docker info >/dev/null 2>&1 || echo "Docker is not running; cross cannot build the Linux targets"
./scripts/build-npm.sh --all
node ./npm/bin/gale.cjs --version      # the launcher is gale.cjs, not bin/gale; must print "gale $VERSION"
ls -l npm/bin/*/gale                   # four files, all freshly mtimed
git status --short                     # exactly the four npm/bin/*/gale files
```

If the launcher still prints the old version, the Cargo substitution in step 2 did not land; fix it and rebuild. npm can never reuse a version number once it is published.

Fold the binaries into the release commit, push, and publish. npm 11 satisfies the two-factor check with **web auth**: `npm publish` opens a browser URL and polls until the user approves, so it runs from a non-interactive shell given the 10-minute timeout.

```bash
npm whoami                             # E401 means an expired token: ask the user to run `! npm login`
npm access list collaborators "$PKG"   # whoami MUST appear with read-write
git add npm/bin
git commit --amend --no-edit
git tag -f -a "v$VERSION" -m "$VERSION"
git push origin HEAD
(cd npm && npm publish --access public)   # opens the browser for 2FA approval; wait for it
```

Push the tag only if the user wants it on origin: with the workflow disabled it triggers nothing, and once the workflow is enabled a pushed tag publishes. Then verify npm as in step 8.

## Common mistakes

| Mistake | Fix |
|---|---|
| Treating "npm version patch" as not a release | It is one. Compute `VERSION` from the registry and run every step. |
| Computing the bump from `package.json` | A half-finished release leaves manifests ahead of the registry; `bun info` is the baseline. |
| Cutting a `release/X` branch or opening a PR | Commit on the current branch. |
| Bumping only the four manifests | Docs, `open_prs.py`, and `docs/index.html` carry the version too. Run the sweep; it must print nothing. |
| Skipping `cargo update --workspace` | `Cargo.lock` keeps the old gale versions and the version guard rejects the tag. |
| Pushing the tag before the dry run | A failed tag run needs a moved tag. Dry-run first; push the tag once it is green. |
| Rebuilding `npm/bin` on the CI path | CI builds every binary from the tag. Local builds are the fallback only. |
| Re-tagging to retry a failed publish | Re-run the failed jobs; every publish step skips what is already out. |
| Using the local fallback while CI is set up | It ships no Windows binaries. Use CI whenever step 1 says it is active. |
| Checking `bun info` once right after publish | The registry takes minutes to process; poll. |
