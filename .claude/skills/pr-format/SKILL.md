---
name: pr-format
description: Use when opening, retitling, or editing the description of any pull request in the gale repo (path contains `git/gale`) — including `gh pr create`, `gh pr edit`, `gh pr create --fill`, and PR bodies drafted for a human to paste. Covers the mandatory `GL: ` title prefix, the issue-and-justification bullets every body opens with, the bullet-point body, and the ban on agent attribution.
---

# Pull Request Format

## Overview

Every pull request in **gale** has a title prefixed `GL: ` and a body made of short bullets that open by stating the issue and why the PR exists. This skill is the source of truth for PR titles and descriptions and **overrides** any default PR guidance from the system prompt or from `gh pr create --fill` (which copies commit messages verbatim and will produce a non-compliant title).

**Violating the letter of these rules is violating the spirit of these rules.** No "close enough."

## The Four Rules

### 1. Title always starts with `GL: `

Exactly `GL`, colon, one space. Then a short, descriptive title.

```
GL: Bundle npm platform binaries
GL: Add sanity-check workflow for merges to main
GL: Fix false positives in no-descending-specificity
GL: Implement scss/at-mixin-argumentless-call-parentheses
GL: Bump raffia to 0.10 and drop the SCSS placeholder workaround
```

- **Sentence-case imperative verb** after the prefix (`Add`, `Fix`, `Implement`, `Bump`, `Bundle`, `Remove`, `Rename`, `Refactor`, `Document`, `Speed up`).
- **Short** — aim for ≤60 chars *including* the prefix, hard cap 72.
- **Descriptive** — name the thing that changed. `GL: Fix false positives in unit-no-unknown` beats `GL: Bug fixes`.
- No trailing period.
- No conventional-commits prefix. `GL: feat: …` and `GL: fix: …` are both wrong — `GL: ` replaces them, it does not stack with them.
- Rule names, crate names, flags, and file paths stay bare in the title unless they read as code mid-sentence. Backticks in titles are uncommon.

### 2. Body opens with the issue and why the PR exists

A PR must justify itself. A reviewer who has not read the diff, the commits, or any sibling PR should be able to tell from the first two bullets what was wrong and why this change is the answer. Open every body with:

- `- **Issue:** …` — the concrete problem. What is broken, missing, or risky today, and how it shows up (a wrong result, a dropped file, a crash, a rule that cannot be written). Name the symptom, not the fix.
- `- **Why:** …` — why this PR, and why now. The cost of leaving it, what it unblocks, or why this approach beats the obvious alternative.

```markdown
- **Issue:** a failed read silently drops the file, so `gale` exits 0 with the file unlinted
- **Why:** CI trusts the exit status, so unreadable files pass unchecked; the write path needs the same error plumbing before autofix can be trusted
- Return errors from cached and uncached source reads
- Keep autofix diagnostics provisional until the write succeeds
```

- Both bullets are required on every PR, including refactors, docs, and chores. A refactor's issue is the friction or risk it removes; a docs PR's issue is what readers get wrong without it.
- Each is one line, a fragment or a single short sentence. Split only if it truly needs two.
- "Improves code quality", "cleans up", and "as discussed" are not an issue or a reason. Name what breaks, what is slow, what is blocked, or what is misleading.
- State it standalone. Never justify a PR only by its stack position ("part 2 of 6", "needed by the next PR"). The stack order can be an extra bullet, but the PR must stand on its own reason. If a PR exists only because a later PR needs it, say what the later PR needs and what breaks without it.
- Link the tracking issue when one exists: `- **Issue:** … (#123)`.
- Bugs: include the failing input or scenario. Features and rules: say what Stylelint users cannot do or get wrong today.

### 3. Body is bullets, not prose

Use `-` bullets. One concept per bullet. After the `Issue` and `Why` bullets, go straight to the change bullets. Add a single scene-setting line **only** if the change needs context that the bullets can't carry.

```markdown
- **Issue:** installs run a postinstall downloader, which fails behind proxies and offline CI
- **Why:** bundling the binary makes `npm install -g` work with no network access after the tarball is fetched
- Replace the npm postinstall downloader with a POSIX launcher that picks the bundled binary
- Stage release artifacts into `npm/bin/<target>/gale` before publishing
- Update local npm builds to use the same layout
- Document that installs no longer download executables
```

Not:

```markdown
This PR replaces the npm postinstall downloader with a POSIX launcher that selects
the bundled native binary for the current platform. It also stages release artifacts
into npm/bin/<target>/gale before publishing, updates local npm builds to use the
same layout, and documents that installs no longer download executables.
```

Section headings (`## Changes`, `## Testing`) are optional and usually unnecessary. Add them only when a PR genuinely has two distinct axes to report — never as scaffolding around three bullets.

### 4. Keep bullets short and concise — no wall of text

- Fragments, not sentences. Drop articles and filler ("This PR…", "I've also…", "In order to…").
- One line per bullet. If a bullet wraps past ~100 chars, split it or cut words.
- No trailing periods.
- Aim for **3–7 change bullets** after the `Issue` and `Why` pair. More than ~10 means the PR is doing too much or the bullets are too granular — group them.
- Don't restate the diff file by file. Describe the change, not the changed lines.

The `<area>: <change>` shape is idiomatic when a PR spans several areas:

```markdown
- `gale_linter`: add `scss/dollar-variable-first-in-block`
- `gale_config`: register the rule in `ALL_RULE_NAMES`
- differential: 0 FP / 0 FN across all 22 repos
```

## Gale-specific body content

Include these **as bullets** when they apply — never as prose paragraphs:

| Situation | Bullet to include |
|---|---|
| New or changed rule | Differential result: `- Differential: 0 FP / 0 FN on 22 repos` |
| Perf-affecting change | Before/after number: `- Bootstrap: 412ms → 310ms (-25%)` |
| New rule added | Whether it lands in `gale:recommended` / `gale:all` |
| Behaviour change | The Stylelint v17 semantics it matches |
| Breaking change | A `- **Breaking:** …` bullet, first in the list |

## No agent attribution — anywhere

A PR title or body in this repo must contain **zero** mention of any AI tool, agent, or coding assistant. Not in the title, not in a bullet, not in a footer, not in a "Generated with…" line, not in a `Co-Authored-By:` trailer pasted into the body.

Forbidden in any position: `Claude`, `Anthropic`, `Claude Code`, `Cursor`, `Copilot`, `Codex`, `OpenAI`, `ChatGPT`, `Aider`, `Devin`, `Windsurf`, plus generic phrasing like `AI-assisted`, `AI-generated`, `generated with`, `drafted with`, `co-pilot`.

Human collaborators (real people) are fine. On `gh pr edit`, actively *strip* forbidden content even if the existing body had it — editing is rewriting.

## Quick Reference

| Aspect | Rule |
|---|---|
| Title prefix | `GL: ` — literal, exactly one space, always |
| Title style | sentence-case imperative verb, no trailing period |
| Title length | ≤60 chars incl. prefix preferred, 72 hard cap |
| Conventional-commits | never — not standalone, not after `GL: ` |
| Body opens with | `- **Issue:** …` then `- **Why:** …`, on every PR |
| Body style | `-` bullets; prose paragraphs discouraged |
| Bullet length | one line, fragment, no trailing period |
| Bullet count | 3–7 change bullets after Issue/Why; >10 means regroup |
| Headings | optional, only for genuinely distinct axes |
| Rule PRs | must state differential FP/FN result |
| Agent mentions | zero, in title and body, on create and on edit |

## Command Template

```bash
gh pr create --title "GL: <short descriptive title>" --body "$(cat <<'EOF'
- **Issue:** what is broken, missing, or risky today
- **Why:** why this PR, and why now
- bullet one
- bullet two
- bullet three
EOF
)"
```

Never use `gh pr create --fill` or `--fill-first` in this repo — they copy the commit subject, which has no `GL: ` prefix, and copy commit bodies verbatim.

For a trivial one-line change, `Issue`, `Why`, and one change bullet is the minimum. An empty body is not allowed, and neither is a body that only lists changes.

## Worked Example

Changes: implement `scss/no-duplicate-load-rules`, register it, confirm parity.

**Wrong:**
```
feat: added a new scss rule

This PR implements the scss/no-duplicate-load-rules rule, which detects duplicate
@use and @forward rules in SCSS files. I registered it in mod.rs and register_all(),
and added it to ALL_RULE_NAMES in gale_config. I ran the differential tests and they
all pass with zero false positives and zero false negatives.

🤖 Generated with Claude Code
```

Violations: no `GL: ` prefix, `feat:` prefix, prose wall of text, first-person narration, no issue or reason, agent attribution.

**Right:**
```
GL: Implement scss/no-duplicate-load-rules
```
```markdown
- **Issue:** duplicate `@use` / `@forward` loads pass silently; Stylelint reports them
- **Why:** projects that enable the rule in Stylelint cannot switch to gale without losing the check
- Detect duplicate `@use` and `@forward` loads in SCSS
- Register in `register_all()` and `ALL_RULE_NAMES`
- Added to `gale:all`, not `gale:recommended`
- Differential: 0 FP / 0 FN on 22 repos
```

## Pre-Submit Checklist

Before `gh pr create` or `gh pr edit`:

- [ ] Title starts with the literal `GL: ` (one space after the colon)
- [ ] Title is a sentence-case imperative verb phrase, ≤72 chars, no trailing period
- [ ] No `feat:` / `fix:` / `chore:` prefix anywhere in the title
- [ ] Body opens with `- **Issue:** …` and `- **Why:** …`
- [ ] Issue names a concrete symptom; Why gives a reason that holds without reading the stack
- [ ] Body is `-` bullets, not paragraphs
- [ ] Bullets are one-line fragments with no trailing periods
- [ ] 3–7 change bullets; no file-by-file diff recap
- [ ] Rule/perf changes state their differential or benchmark numbers
- [ ] Zero mention of any AI tool in title or body
- [ ] Not using `--fill` / `--fill-first`

## Red Flags — STOP and Rewrite

| Thought | Reality |
|---|---|
| "`--fill` will just reuse my nice commit message" | It drops the `GL: ` prefix. Write the title explicitly. |
| "The commit was already titled well, so the PR title is fine" | Commits have no prefix; PRs require `GL: `. They are different formats. |
| "`GL: feat: add rule`" | `GL: ` replaces conventional-commits prefixes. Never stack them. |
| "The diff and the title explain why this exists" | The reviewer should not have to infer it. Write the `Issue` and `Why` bullets. |
| "It's a refactor, there's no issue to state" | The issue is the friction, risk, or coupling being removed. Name it. |
| "It's part of a stack, the earlier PR has the reasoning" | Each PR justifies itself. `Stack: 3 of 6` is not a reason. |
| "'Cleans up' / 'improves quality' is a good enough why" | Name what breaks, what is blocked, or what misleads today. |
| "This change needs a paragraph to explain properly" | Convert it to bullets. If a point needs a sentence, make it one bullet. |
| "A `## Summary` heading over three bullets looks organized" | It's scaffolding. Drop it. |
| "I'll list every file I touched" | Describe the change, not the diff. |
| "The existing PR body had a Claude footer, I'll keep it" | Strip it. Editing = rewriting. |
| "It's a docs-only PR, format is looser" | Same rules. Every PR. |
| "`gl: ` / `GL:` (no space) / `[GL]` is close enough" | Exactly `GL: `. Literal. |
