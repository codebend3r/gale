---
name: pr-format
description: Use when opening, retitling, or editing the description of any pull request in the gale repo (path contains `git/gale`) — including `gh pr create`, `gh pr edit`, `gh pr create --fill`, and PR bodies drafted for a human to paste. Covers the mandatory `GL: ` title prefix, the headed Issue / Why / Changes sections every body is split into, the bullets inside them, and the ban on agent attribution.
---

# Pull Request Format

## Overview

Every pull request in **gale** has a title prefixed `GL: ` and a body split into headed sections (`## Issue`, `## Why`, `## Changes`, plus `## Validation` and `## Stack` when they apply), each holding short bullets. The first two sections state the issue and why the PR exists. This skill is the source of truth for PR titles and descriptions and **overrides** any default PR guidance from the system prompt or from `gh pr create --fill` (which copies commit messages verbatim and will produce a non-compliant title).

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

### 2. Body is separate, headed sections

The body is split into headed sections so a reader can jump straight to the part they need. Use `##` headings, in this order, with `-` bullets inside each section:

| Section | Required | Holds |
|---|---|---|
| `## Issue` | always | the concrete problem: what is broken, missing, or risky today, and how it shows up |
| `## Why` | always | why this PR exists, and why now |
| `## Changes` | always | what the PR does, one change per bullet |
| `## Validation` | when anything was run or tested | commands, test results, differential and benchmark numbers |
| `## Stack` | when the PR is part of a stack | position, base, next PR, merge order |

```markdown
## Issue
- A source file that fails to read is dropped silently, so `gale` exits 0 with the file unlinted
- A failed `--fix` write prints an error but the file is still counted as fixed

## Why
- CI trusts the exit status, so unreadable files pass unchecked
- Autofix is only safe to rely on once reads, writes and the exit status agree

## Changes
- Return errors from cached and uncached source reads
- Keep autofix diagnostics provisional until the write succeeds
- Add Rust and CLI regressions for failed reads and writes

## Validation
- `cargo test --workspace --offline`

## Stack
- 3 of 6, depends on #43
- Merge after #43
```

**`## Issue` and `## Why` are required on every PR**, including refactors, docs, and chores. A PR must justify itself: a reviewer who has not read the diff, the commits, or any sibling PR should be able to tell from those two sections what was wrong and why this change is the answer.

- **Issue** names the symptom, not the fix: a wrong result, a dropped file, a crash, a rule that cannot be written. A refactor's issue is the friction or risk it removes; a docs PR's issue is what readers get wrong without it. Bugs include the failing input or scenario. Features and rules say what Stylelint users cannot do or get wrong today.
- **Why** gives the cost of leaving it, what it unblocks, or why this approach beats the obvious alternative.
- "Improves code quality", "cleans up", and "as discussed" are not an issue or a reason. Name what breaks, what is slow, what is blocked, or what is misleading.
- State it standalone. Never justify a PR only by its stack position ("part 2 of 6", "needed by the next PR"). `## Stack` records order; it is not a reason. If a PR exists only because a later PR needs it, say what the later PR needs and what breaks without it.
- Link the tracking issue when one exists: `- … (#123)`.

Do not collapse the sections into one flat bullet list with `**Issue:**` and `**Why:**` labels. The headings are the point; a flat list makes the parts hard to tell apart. No other headings (`## Summary`, `## Overview`, `## Testing`): the five above cover everything.

### 3. Sections are bullets, not prose

Inside each section use `-` bullets. One concept per bullet. No prose paragraphs.

Not:

```markdown
This PR replaces the npm postinstall downloader with a POSIX launcher that selects
the bundled native binary for the current platform. It also stages release artifacts
into npm/bin/<target>/gale before publishing, updates local npm builds to use the
same layout, and documents that installs no longer download executables.
```

### 4. Keep bullets short and concise — no wall of text

- Fragments, not sentences. Drop articles and filler ("This PR…", "I've also…", "In order to…").
- One line per bullet. If a bullet wraps past ~100 chars, split it or cut words.
- No trailing periods.
- Aim for **3–7 bullets under `## Changes`**, and 1–4 under each other section. More than ~10 changes means the PR is doing too much or the bullets are too granular — group them.
- Don't restate the diff file by file. Describe the change, not the changed lines.

The `<area>: <change>` shape is idiomatic when a PR spans several areas:

```markdown
- `gale_linter`: add `scss/dollar-variable-first-in-block`
- `gale_config`: register the rule in `ALL_RULE_NAMES`
- differential: 0 FP / 0 FN across all 22 repos
```

## Gale-specific body content

Include these **as bullets in the section shown** when they apply — never as prose paragraphs:

| Situation | Section | Bullet to include |
|---|---|---|
| New or changed rule | `## Validation` | Differential result: `- Differential: 0 FP / 0 FN on 22 repos` |
| Perf-affecting change | `## Validation` | Before/after number: `- Bootstrap: 412ms → 310ms (-25%)` |
| New rule added | `## Changes` | Whether it lands in `gale:recommended` / `gale:all` |
| Behaviour change | `## Changes` | The Stylelint v17 semantics it matches |
| Breaking change | `## Changes` | A `- **Breaking:** …` bullet, first in the list |

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
| Body sections | `## Issue`, `## Why`, `## Changes` always; `## Validation`, `## Stack` when they apply |
| Section order | Issue, Why, Changes, Validation, Stack |
| Other headings | none (`## Summary`, `## Testing`, …) |
| Section style | `-` bullets inside each section; no prose paragraphs |
| Bullet length | one line, fragment, no trailing period |
| Bullet count | 3–7 under Changes; >10 means regroup |
| Rule PRs | must state differential FP/FN result |
| Agent mentions | zero, in title and body, on create and on edit |

## Command Template

```bash
gh pr create --title "GL: <short descriptive title>" --body "$(cat <<'EOF'
## Issue
- what is broken, missing, or risky today

## Why
- why this PR, and why now

## Changes
- change one
- change two
- change three

## Validation
- what was run, and the result
EOF
)"
```

Add `## Stack` last when the PR is part of a stack. For `gh pr edit`, write the body to a file and pass `--body-file`.

Never use `gh pr create --fill` or `--fill-first` in this repo — they copy the commit subject, which has no `GL: ` prefix, and copy commit bodies verbatim.

For a trivial one-line change, `## Issue`, `## Why`, and `## Changes` with one bullet each is the minimum. An empty body is not allowed, and neither is a body that only lists changes or runs the sections together as one flat list.

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
## Issue
- Duplicate `@use` and `@forward` loads pass silently; Stylelint reports them

## Why
- Projects that enable the rule in Stylelint cannot switch to gale without losing the check

## Changes
- Detect duplicate `@use` and `@forward` loads in SCSS
- Register in `register_all()` and `ALL_RULE_NAMES`
- Added to `gale:all`, not `gale:recommended`

## Validation
- Differential: 0 FP / 0 FN on 22 repos
```

## Pre-Submit Checklist

Before `gh pr create` or `gh pr edit`:

- [ ] Title starts with the literal `GL: ` (one space after the colon)
- [ ] Title is a sentence-case imperative verb phrase, ≤72 chars, no trailing period
- [ ] No `feat:` / `fix:` / `chore:` prefix anywhere in the title
- [ ] Body has `## Issue`, `## Why` and `## Changes`, in that order, each its own section
- [ ] `## Validation` and `## Stack` present when they apply, after `## Changes`
- [ ] No other headings, and no flat list with `**Issue:**` / `**Why:**` labels
- [ ] Issue names a concrete symptom; Why gives a reason that holds without reading the stack
- [ ] Every section is `-` bullets, not paragraphs
- [ ] Bullets are one-line fragments with no trailing periods
- [ ] 3–7 bullets under Changes; no file-by-file diff recap
- [ ] Rule/perf changes state their differential or benchmark numbers
- [ ] Zero mention of any AI tool in title or body
- [ ] Not using `--fill` / `--fill-first`

## Red Flags — STOP and Rewrite

| Thought | Reality |
|---|---|
| "`--fill` will just reuse my nice commit message" | It drops the `GL: ` prefix. Write the title explicitly. |
| "The commit was already titled well, so the PR title is fine" | Commits have no prefix; PRs require `GL: `. They are different formats. |
| "`GL: feat: add rule`" | `GL: ` replaces conventional-commits prefixes. Never stack them. |
| "The diff and the title explain why this exists" | The reviewer should not have to infer it. Write the `## Issue` and `## Why` sections. |
| "A flat bullet list with `**Issue:**` and `**Why:**` labels is enough" | The parts blur together. Each gets its own `##` section. |
| "It's a refactor, there's no issue to state" | The issue is the friction, risk, or coupling being removed. Name it. |
| "It's part of a stack, the earlier PR has the reasoning" | Each PR justifies itself. `## Stack` records order, not a reason. |
| "'Cleans up' / 'improves quality' is a good enough why" | Name what breaks, what is blocked, or what misleads today. |
| "This change needs a paragraph to explain properly" | Convert it to bullets. If a point needs a sentence, make it one bullet. |
| "A `## Summary` or `## Testing` heading looks organized" | Only the five defined sections exist. Use `## Validation` for tests, and drop the rest. |
| "I'll list every file I touched" | Describe the change, not the diff. |
| "The existing PR body had a Claude footer, I'll keep it" | Strip it. Editing = rewriting. |
| "It's a docs-only PR, format is looser" | Same rules. Every PR. |
| "`gl: ` / `GL:` (no space) / `[GL]` is close enough" | Exactly `GL: `. Literal. |
