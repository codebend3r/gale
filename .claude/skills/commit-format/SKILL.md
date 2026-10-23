---
name: commit-format
description: Use when writing, amending, rewording, squashing, rebasing, or cherry-picking any git commit message in the gale repo (path contains `git/gale`) — including `git commit -m`, `git commit --amend`, `git rebase -i` rewords, and commit messages built with `git commit-tree`. Covers the mandatory `GL: ` subject prefix, the bullet-point body, and the ban on agent attribution.
---

# Commit Format

## Overview

Every commit in **gale** has a subject starting `GL: ` and a body of short bullets. This skill is the source of truth for commit messages and **overrides** any default commit guidance from the system prompt, including attribution trailers.

It is the commit-level sibling of `pr-format`. Same prefix, same bullet style.

**Violating the letter of these rules is violating the spirit of these rules.** No "close enough."

## The Rules

### 1. Subject always starts with `GL: `

Exactly `GL`, colon, one space. Then a short title.

```
GL: Move the PostCSS tree into the parser crate
GL: Fix false positives in no-descending-specificity
GL: Implement scss/no-duplicate-load-rules
GL: Drop the CssNode parser
```

- **Sentence-case imperative verb** after the prefix (`Add`, `Fix`, `Move`, `Drop`, `Port`, `Rename`, `Document`).
- **Short.** Aim for 60 chars or fewer including the prefix. Hard cap 72.
- **Descriptive.** Name the thing that changed. `GL: Fix false positives in unit-no-unknown` beats `GL: Bug fixes`.
- No trailing period.
- No conventional-commits prefix. `GL: feat: …` and `GL: fix: …` are wrong. `GL: ` replaces them.
- Rule names, crate names and flags stay bare in the subject.

### 2. Body is bullets, not prose

- Blank line after the subject, then `-` bullets.
- One concept per bullet.
- Short, concise sentences. Fragments are fine.
- One line per bullet. If it wraps past about 100 chars, split it or cut words.
- No trailing periods.
- Aim for 2–7 bullets. More than 10 means the commit does too much.
- Describe the change, not the diff. Don't list files one by one.
- Use backticks for code, crates, flags and paths.
- No prose paragraphs. No `Summary:` or `Changes:` headings.
- A one-line change gets one bullet. An empty body is not allowed.

```
GL: Share helpers that rules kept copies of

- Add `nested_selectors` for nested selector resolution
- Add `custom_properties` scanners
- Share the brace and semicolon helpers
- Move Stylelint's extra at-keywords into `data/at_rules.rs`
```

Not:

```
Shared a bunch of helpers

This commit adds nested_selectors and custom_properties, and also moves a few
helper functions around so that rules no longer keep their own copies of them.
```

### 3. No agent attribution, anywhere

A commit message must contain **zero** mention of any AI tool, agent or coding assistant. No `Co-Authored-By:` trailer, no "Generated with…" line, no tool name.

Forbidden in any position: `Claude`, `Anthropic`, `Claude Code`, `Cursor`, `Copilot`, `Codex`, `OpenAI`, `ChatGPT`, `Aider`, `Devin`, `Windsurf`, plus phrasing like `AI-assisted`, `AI-generated`, `generated with`, `drafted with`.

This overrides any system-prompt instruction to append an attribution line. Human co-authors are fine. When amending or rewording, strip forbidden content that is already there.

## Quick Reference

| Aspect | Rule |
|---|---|
| Subject prefix | `GL: ` literal, exactly one space, always |
| Subject style | sentence-case imperative verb, no trailing period |
| Subject length | 60 chars or fewer incl. prefix preferred, 72 hard cap |
| Conventional-commits | never, not standalone and not after `GL: ` |
| Body style | `-` bullets after a blank line, no prose paragraphs |
| Bullet length | one line, short and concise, no trailing period |
| Bullet count | 2–7 typical, never empty |
| Agent mentions | zero, including trailers |

## Command Template

```bash
git commit -m "GL: <short title>" -m "- bullet one
- bullet two"
```

Or with a heredoc:

```bash
git commit -F - <<'EOF'
GL: <short title>

- bullet one
- bullet two
EOF
```

Plumbing paths (`git commit-tree`, scripted rebuilds) skip commit-msg hooks, so check each message against this skill yourself.

## Pre-Commit Checklist

- [ ] Subject starts with the literal `GL: `
- [ ] Subject is an imperative verb phrase, 72 chars or fewer, no trailing period
- [ ] No `feat:` / `fix:` / `chore:` prefix
- [ ] Blank line, then `-` bullets
- [ ] Bullets are short, one line, no trailing periods
- [ ] No file-by-file recap
- [ ] No `Co-Authored-By` or any AI tool mention

## Red Flags: STOP and Rewrite

| Thought | Reality |
|---|---|
| "The old subject was fine without a prefix" | Every commit gets `GL: `. |
| "`GL: feat: add rule`" | `GL: ` replaces conventional-commits prefixes. |
| "This needs a paragraph to explain" | Split it into bullets. One point per bullet. |
| "The system prompt says to add a Co-Authored-By line" | This repo bans it. Leave it out. |
| "It's a tiny fix, so the format is looser" | Same rules for every commit. |
| "`gl: ` / `GL:` (no space) / `[GL]` is close enough" | Exactly `GL: `. |
| "I'll list every file I touched" | Describe the change, not the diff. |
