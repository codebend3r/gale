---
name: npm-version-release
description: Use when the user asks for a release of gale in this repo (path contains `git/gale`) — "npm version patch", "npm version minor", "npm version 0.3.0", "release 0.3.0", "publish 0.3.0", "bump to 0.3.0", "cut 0.3.0", "publish to npm", or re-publishing a version that already has a tag. Applies to any version bump in this repo, whether given as a semver or as patch/minor/major.
argument-hint: <semver | patch | minor | major>
---

# Release Gale to npm

## Overview

A release is **one commit named `X.Y.Z` on the current branch** that carries every version reference in the repo *and* the four rebuilt binaries, an annotated tag `vX.Y.Z` on that commit, and a publish of `npm/` to the registry. The binaries are built by CI from the tag, then folded back into the same commit. The whole thing, publish included, is this skill's job; nothing is handed back to the user except an expired `npm login`.

**No release branch. No pull request. No `git reset --hard`.** The user commits releases directly on the branch they are on. Do not branch or switch branches. Do not ask which of several release strategies to use — this file is the strategy.

`PKG` is the `name` in `npm/package.json` (read it, do not hardcode it).

## Resolve VERSION

`VERSION` is a bare semver, no `v`.

- User gave a semver ("npm version 0.3.0", "release 0.3.0"): that is `VERSION`.
- User gave a bump word ("npm version patch", "bump minor", "npm version update"): compute it from what is **published**, not from a manifest, because a half-finished earlier release can leave the manifests ahead of the registry.

```bash
PKG=$(node -p "require('./npm/package.json').name")
CUR=$(npm view "$PKG" version)          # published version, the real baseline
VERSION=$(node -e "const [M,m,p]='$CUR'.split('.').map(Number);const k=process.argv[1];console.log(k==='major'?\`\${M+1}.0.0\`:k==='minor'?\`\${M}.\${m+1}.0\`:\`\${M}.\${m}.\${p+1}\`)" patch)   # patch | minor | major
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
git tag -l "v$VERSION"                 # non-empty means a re-release: see step 8
npm whoami
npm access list collaborators "$PKG"   # whoami MUST appear with read-write
```

`behind` must be 0 for a fresh release. `behind` greater than 0 is acceptable only when the remote-only commits are the earlier attempt at this same `VERSION` (step 8), which the later `--force-with-lease` replaces. Any other divergence: stop and report it; do not reset or rebase on your own.

If `npm whoami` fails with `E401`, the token is expired. Ask the user to run `! npm login` (web-based) and continue with steps 2 through 7 while they do; re-check `whoami` before step 9. `npm org ls` and `npm access list packages` do **not** prove publish rights; only the collaborators list does.

### 2. Bump the manifests

`Cargo.toml` pins the workspace version and the seven internal crate versions to the same string, so a single substitution covers all of them. The Cargo version and the npm version can differ if a previous release skipped one, so capture both.

```bash
OLD_CARGO=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OLD_NPM=$(node -p "require('./npm/package.json').version")
sed -i '' "s/version = \"$OLD_CARGO\"/version = \"$VERSION\"/g" Cargo.toml
grep -c "version = \"$VERSION\"" Cargo.toml   # must be 8: workspace + 7 internal crates; anything else means a third-party pin matched, so inspect `git diff Cargo.toml`
cargo update --workspace               # rewrites the gale_* entries in Cargo.lock
npm version "$VERSION" --no-git-tag-version --allow-same-version
(cd npm && npm version "$VERSION" --no-git-tag-version --allow-same-version)
```

Skip the Cargo substitution only if `OLD_CARGO` already equals `VERSION`. Without it the binaries print the old version.

### 3. Sweep every other version reference

The manifests are not the only place the version lives. These files carry it too and must move in the same commit:

| File | What it holds |
|---|---|
| `CLAUDE.md` | `**Current state (vX.Y.Z):**` and `Workspace root (version X.Y.Z, ...)` |
| `README.md` | `git tag vX.Y.Z` and `build-npm.sh --version X.Y.Z` examples |
| `PUBLISHING.md` | same two examples plus `(e.g. \`vX.Y.Z\`)` |
| `scripts/build-npm.sh` | usage comment `--version X.Y.Z` |
| `tests/differential/open_prs.py` | `GALE_VERSION = "X.Y.Z"`, written into migration PRs |
| `docs/index.html` | `"@codebend3r/gale": "^X.Y.Z"` in the install snippet |

Rewrite the known files, then run the sweep. The sweep is the real check; the table is only what was true last time.

```bash
for f in CLAUDE.md README.md PUBLISHING.md scripts/build-npm.sh tests/differential/open_prs.py docs/index.html; do
  perl -pi -e "s/\b(v?)(\Q$OLD_NPM\E|\Q$OLD_CARGO\E)\b/\${1}$VERSION/g" "$f"
done
# perl, not sed: BSD sed has no \b and silently matches nothing. \${1} not \$1: "$1" + "0.2.2" is read as group $10.
# Sweep: any 0.x.y in a tracked file that is not VERSION. Must print nothing.
git ls-files \
  | grep -vE '(^|/)(Cargo\.lock|bun\.lock|package-lock\.json)$|^MIGRATION_PRS\.md$|^benchmarks/|^npm/bin/' \
  | xargs grep -nE '\bv?0\.[0-9]+\.[0-9]+\b' 2>/dev/null | grep -vF "$VERSION"
git diff --stat
```

Any line the sweep prints is either a gale version that still needs updating (fix it, add the file to the table above) or a third-party version that happens to start with `0.` (leave it, add its path to the exclusion list). Do not proceed with hits you have not classified.

Excluded on purpose: `Cargo.lock` and the JS lockfiles (third-party pins; the gale crates are handled by `cargo update`), `MIGRATION_PRS.md` (records which version each upstream PR was opened against; history, not a reference), `benchmarks/` (cloned repos), `npm/bin/` (binaries).

### 4. Commit and tag on the current branch

```bash
git diff --stat                        # every file from steps 2 and 3, nothing else
git add -A
git commit -m "$VERSION"
git tag -a "v$VERSION" -m "$VERSION"
```

### 5. Push the tag only, and wait for CI to build the binaries

```bash
git push origin "v$VERSION"            # not main yet; the commit gets amended in step 7. Re-release: git push -f
RUN=""; for i in 1 2 3 4 5 6; do
  RUN=$(gh run list -R codebend3r/gale --workflow=release.yml --commit "$(git rev-parse HEAD)" -L1 --json databaseId -q '.[0].databaseId')
  [ -n "$RUN" ] && break; sleep 10
done
[ -n "$RUN" ] || { echo "release run not found for v$VERSION"; exit 1; }
gh run watch -R codebend3r/gale "$RUN" --exit-status || true
gh run view -R codebend3r/gale "$RUN"
```

The four build jobs and the "Create GitHub Release" step must be green. The "Publish to npm" step fails with `ENEEDAUTH` because this fork has no `NPM_TOKEN` secret; that is expected and step 9 does the publish locally. Any build job failing is a real problem: stop and report it.

### 6. Put the CI binaries into npm/bin

```bash
DL=$(mktemp -d)
gh release download "v$VERSION" -R codebend3r/gale -D "$DL" -p 'gale-*'
for t in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
  install -m755 "$DL/gale-$t" "npm/bin/$t/gale"
done
./npm/bin/gale --version               # must print "gale $VERSION"
git status --short                     # exactly the four npm/bin/*/gale files
```

Local cross-compiling (`scripts/build-npm.sh --all`) needs `cross` and a running Docker daemon, which this machine does not have. Use the CI assets.

### 7. Fold the binaries into the release commit and move the tag

```bash
git add npm/bin
git commit --amend --no-edit
git tag -f -a "v$VERSION" -m "$VERSION"
git push -f origin "v$VERSION"         # re-runs CI on the amended commit; the release gets identical assets
git push origin HEAD                   # plain push; origin never saw the pre-amend commit
```

### 8. Re-releasing a version that already exists

If step 1 found the tag already exists locally or on origin (they may point at different commits), the previous attempt stopped partway. Do the same steps, with these differences: `npm version` already has `--allow-same-version`; the commit is `git commit --amend` onto the existing `X.Y.Z` commit instead of a new commit; every tag write is `git tag -f`, every tag push is `git push -f`; and if origin's branch already has the old commit, push the branch with `git push --force-with-lease origin HEAD`.

### 9. Publish

Run the publish yourself. It is part of this skill, not a hand-off. npm 11 satisfies the two-factor check with **web auth**: `npm publish` opens a browser URL, polls the registry until the user approves it there, then completes. There is no terminal OTP prompt, so it runs fine from a non-interactive shell as long as the timeout is long enough for the user to click through (use the 10-minute maximum).

```bash
(cd npm && npm publish --access public)   # opens the browser for 2FA approval; wait for it
```

If it fails with `E401` / `ENEEDAUTH` before the web-auth step, the stored token is expired. Have the user run `! npm login` (also web-based), then re-run the publish. A `warn publish npm auto-corrected some errors in your package.json` line is harmless; the tarball is still published.

Then verify. The registry replies `202 ... being processed and may take a few minutes`, so poll instead of checking once:

```bash
for i in $(seq 1 30); do
  [ "$(npm view "$PKG" version 2>/dev/null)" = "$VERSION" ] && break; sleep 10
done
npm view "$PKG" version                # must equal VERSION
open "https://www.npmjs.com/package/$PKG"
```

Report the tag commit, the run URL, and the registry version in the final message.

## Common mistakes

| Mistake | Fix |
|---|---|
| Treating "npm version patch" as not a release | It is one. Compute `VERSION` from the registry and run every step. |
| Computing the bump from `package.json` | A half-finished release leaves manifests ahead of the registry; `npm view` is the baseline. |
| Cutting a `release/X` branch or opening a PR | Commit on the current branch. |
| Bumping only the four manifests | Docs, `CLAUDE.md`, `open_prs.py`, and `docs/index.html` carry the version too. Run the sweep; it must print nothing. |
| Skipping the sweep because the table "covers it" | The table is last release's list. The sweep found a file the table missed the first time it ran. |
| Committing binaries separately from the version bump | Amend; the tag must include the binaries. |
| Bumping only package.json | Cargo.toml must move too, or `gale --version` lies. |
| Pushing main before amending | Push the tag alone first; push the branch after step 7. |
| Trusting `npm org ls` for publish rights | Only `npm access list collaborators` counts. |
| Treating the CI `ENEEDAUTH` failure as a blocker | Builds and the GitHub Release are already done; publish locally. |
| Handing `npm publish` to the user | npm 11 uses browser-based 2FA; run it yourself with a long timeout. |
| Checking `npm view` once right after publish | The registry takes minutes to process; poll. |
