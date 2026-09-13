---
name: git-branch-naming
description: Use when choosing or typing a git branch name in this repo (path contains `git/gale`) — `git switch -c`, `git checkout -b`, `git branch`, `git worktree add`, `gh pr create --head`, or naming a branch in a plan or message before it exists.
---

# Naming a branch

## The shape

A branch name is **one to five kebab-case words that say what the branch is for**. Nothing else.

```
git switch -c fixing-broken-tests
git switch -c refactoring-text-formatter
git switch -c release-0-2-2
git switch -c sass-indented-syntax
git switch -c readme-benchmark-numbers
git switch -c specificity-empty-selector-panic
```

## Rules

- **Flat.** No slash anywhere. `fix/`, `feat/`, `feature/`, `bug/`, `hotfix/`, `chore/`, `docs/`, `refactor/`, `release/`, a username, a ticket folder: all of these are folders, and folders are banned. The type of work goes into the words or nowhere.
- **kebab-case.** Lowercase ASCII letters, digits, hyphens. Dots become hyphens (`0.2.2` → `0-2-2`). No underscores, no capitals.
- **1 to 5 words.** Count the hyphen-separated parts. Six is too many; cut adjectives and issue numbers first, not the noun that says what the branch touches.
- **Describes the work.** A reader should know what the branch is for from the name alone. `wip`, `test`, `changes`, `cj-branch` say nothing.

## Trimming to five words

Start from what you would have written, drop the folder, then drop words in this order until five remain: ticket/issue numbers, "and"/"the"/"for", adjectives, the verb's `-ing` form if a noun phrase still reads.

| Too long | Trimmed |
|---|---|
| `hotfix/selector-max-specificity-empty-selector-panic` | `specificity-empty-selector-panic` |
| `refactor/text-formatter-share-code-with-compact-formatter` | `refactoring-text-formatter` |
| `feature/add-support-for-sass-indented-syntax` | `sass-indented-syntax` |

## Red flags

If the name you are about to type contains a `/`, stop. That is the folder habit, and it is wrong here every time, including for hotfixes and releases. Remove the folder and re-count the words.
