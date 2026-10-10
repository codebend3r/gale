# Feature research: what would win Stylelint users

Which features would add the most value to Gale and convince more people to
use it as their CSS and SCSS linter. Researched on **2026-10-09** from Reddit,
Hacker News, YouTube, GitHub (Stylelint's issue tracker and changelog, Gale's
own repo) and web search. Stylelint discussion is sparse, so Reddit and Hacker
News were searched over roughly four months instead of 30 days. X/Twitter was
not searched.

Legend for the catalog below:

| Mark | Meaning |
|------|---------|
| ★ | Recommended first |
| Evidence | Someone asked for it, or the research points straight at it |
| Idea | Inferred; no direct demand found |

## Where things stand

| | Stylelint | Gale |
|---|---|---|
| Pitch | "A mighty CSS linter that helps you avoid errors and enforce conventions" | "An extremely fast CSS linter. Drop-in replacement for Stylelint" |
| GitHub stars | 11,528 | 0 |
| Latest release | 17.16.0 (2026-10-01), which its changelog calls "likely the last 17.x release, as we prepare for 18.0.0" | 0.2.5 |
| Community mentions | About 10 Reddit comments and 3 HN comments in four months | None found |
| Speed | 0.6s to 24s on the benchmark repos | 11x to 122x faster on the same repos |

Other tools in the space:

- **Biome** has not shipped SCSS linting. Its
  [language support page](https://biomejs.dev/internals/language-support/)
  lists SCSS linting as unsupported, and its
  [2026 roadmap](https://biomejs.dev/blog/roadmap-2026/) calls SCSS its most
  wanted feature. Parser work is still landing in October 2026
  ([#8732](https://github.com/biomejs/biome/issues/8732)).
- **csskit** is a Rust CSS toolchain (lint, format, LSP) inspired by oxc. Its
  own README calls it alpha quality.
- **Stylelint v18** is under way on a `v18` branch
  ([#9338](https://github.com/stylelint/stylelint/issues/9338)): it drops
  Node 20, removes `context.fix`, and adds `value-no-invalid` and
  `declaration-property-custom-property-allowed-list`.
- **Dart Sass** deprecated `@import` in 1.80.0 on 2024-10-17 and said removal
  would come no sooner than two years later, in Dart Sass 3.0.0
  ([Sass blog](https://sass-lang.com/blog/import-is-deprecated/)). The
  earliest removal date is 2026-10-17. The latest release at the time of
  writing is 1.105.1 (2026-09-29).

## What people are saying

**Nobody complains that Stylelint is slow.** Speed alone will not move
people; Gale needs things Stylelint does not do.

**Linting is how people keep AI-written CSS in line.** The most-discussed CSS
thread found, r/css's
["Why is Claude Code so bad at writing CSS?"](https://reddit.com/comments/1v28uj9)
(71 comments), keeps landing on a strict Stylelint config as the fix:

> Distill your specific guardrails and preferred implementations, then lint
> [...] whatever it produces. It's what stylelint was made for.
>
> u/getsiked, r/css, 2026-07-21

> stylelint with an allowed-list so raw hex and rogue margins just fail, all
> shipped as one npm config package you drop into each repo. The agent
> actually fixes its own violations because the errors land in its loop,
> while CLAUDE.md tbh just falls out of context once the session gets long.
>
> [u/Zealousideal-Ebb-355](https://reddit.com/comments/1u2z984/_/or1bwfn), r/Frontend, 2026-06-11

**Design-token enforcement is Stylelint's best-known use**, enough to be a
joke on Hacker News: "stylelint beeps you can't just pass hex colors
directly, that color is not in our design system"
([glerk](https://news.ycombinator.com/item?id=48812271), 2026-07-07). A
spacing-scale plugin, Rhythmguard, also launched on HN in April.

**People want to know when a declaration does nothing.** The NoEffect VS Code
extension ([r/css](https://reddit.com/comments/1vuhw0b), 170 points) flags
things like `justify-content` on an element that is not a flex or grid
container. One commenter asked for exactly what Gale could provide:

> Have you thought about exposing the same engine through a CLI or Stylelint
> integration eventually? If teams could get the same NoEffect findings in VS
> Code locally and then run them in CI on a PR, this could become much more
> useful for larger codebases
>
> [u/GeekyAntsFlutterdev](https://reddit.com/comments/1vuhw0b/_/p5ke905), 2026-08-24

Commenters also criticized it for flagging normal cascade overrides ("That's
just how CSS _should_ work"), so any version of this has to avoid false
positives.

**Baseline is expected.** "Commonly used tools like Browserslist and
Stylelint support checking for Baseline compliance"
([alwillis on HN](https://news.ycombinator.com/item?id=49267909), 2026-08-12).

**Nesting depth still hurts.** "You have to use stylelint and set a max
nesting level just to keep it sane. I had a project where we were nesting 13
levels deep"
([u/paceaux](https://reddit.com/comments/1wpgmoc/_/pcmkbi6), r/css,
2026-09-28).

**Teams have moved JavaScript tooling to Rust but not CSS.** PostHog's repo
([HN](https://news.ycombinator.com/item?id=48848915)) has `.oxlintrc.json`
and `.oxfmtrc.json` next to a `.stylelintrc.js`. Teams like this are the most
likely to adopt Gale, and their configs are often JavaScript.

## Recommended first

1. **An AI-agent guardrail mode** (catalog #1 to #3). A strict preset, output
   short enough for an agent to act on, and a ready-made editor hook. This is
   where the strongest demand is, and where speed matters most: a linter that
   runs after every agent edit cannot take five seconds.
2. **No-effect declaration rules** (#4). Stylelint has no core rule for this,
   and people asked for it in CI.
3. **Dart Sass 3 readiness with autofix** (#12 to #15). The `@import` removal
   window opens on 2026-10-17, and Biome cannot lint SCSS yet.
4. **Baseline targets** (#5).
5. **Fix what blocks migration** (group C), starting with v17 message wording,
   v18 behavior and JavaScript configs.

## Full catalog

### A. Features Stylelint doesn't have

| # | Feature | Why |
|--:|---------|-----|
| 1 | ★ `gale:strict` preset for AI agents: no raw hex outside design tokens, a fixed spacing scale, no `!important`, a nesting-depth cap, no ID selectors | Evidence: the AI-agent threads above. Builds on `plugin/enforce-variable-for-property` |
| 2 | ★ Output an agent can act on: file:line, rule and a one-line fix hint, kept short | Evidence: the same threads |
| 3 | ★ Ready-made Claude Code and Cursor hooks that run `gale --fix` on edited style files | Evidence: the same threads; Gale's speed is what makes this practical |
| 4 | ★ No-effect declaration rules, for example `justify-content` or `gap` alongside `display: block`, `z-index` or `top` alongside `position: static`, `width` on `display: inline`, `float` alongside `position: absolute`. Only flag cases visible in a single block | Evidence: NoEffect post (170 points) and the request for a CLI version for CI |
| 5 | ★ Baseline targets in `plugin/browser-compat`, such as `"baseline": "widely"`, with the data bundled instead of read from `node_modules` | Evidence: HN comment treating Baseline checks as standard |
| 6 | WASM build and an online playground | Evidence: Stylelint's "Add support for running in a browser" ([#3935](https://github.com/stylelint/stylelint/issues/3935), 25 comments) is still open |
| 7 | Specificity and nesting report, for example a `--report specificity` flag that lists the worst selectors | Evidence: nesting 13 levels deep, r/css |
| 8 | "Ready for native CSS" preset that flags SCSS features with native equivalents (nesting, `$var` that could be custom properties) | Thin evidence: r/css "Is SCSS still worth learning in 2026" thread |
| 9 | `github` formatter for GitHub Actions annotations, which Stylelint removed in v17 | Idea |
| 10 | SARIF formatter for GitHub code scanning | Idea |
| 11 | Run Gale from inside ESLint or oxlint | Evidence: "Become an ESLint plugin" is Stylelint's most-upvoted open issue ([#6593](https://github.com/stylelint/stylelint/issues/6593), 23 reactions) |

### B. SCSS and Dart Sass 3

| # | Feature | Why |
|--:|---------|-----|
| 12 | ★ `gale:sass3` preset that flags `@import` and steers toward `@use` | Evidence: `@import` removal window opens 2026-10-17 |
| 13 | ★ Autofix for `scss/no-global-function-names`: rewrite `map-get(...)` to `map.get(...)` and add the `@use "sass:map"` line | Evidence: the same deprecation; Gale's rule has no fix today |
| 14 | ★ Module-system rules: `scss/at-use-no-unnamespaced`, `scss/at-use-no-redundant-alias`, `scss/no-duplicate-load-rules`, `scss/dollar-variable-no-namespaced-assignment` | Evidence: missing from Gale, present in stylelint-scss |
| 15 | ★ Sass color rules: `scss/function-color-relative`, `scss/function-color-channel` | Evidence: Dart Sass deprecates the global color functions |
| 16 | The rest of the 28 stylelint-scss rules Gale lacks (listed below) | Configs that use them get the rules skipped with a warning |
| 17 | Autofix in `.sass` files | Problems are reported today, but `--fix` leaves the file unchanged |

stylelint-scss has 71 rules; Gale is missing 28. Besides those in #14 and
#15:

- `no-unused-private-members`, `property-no-unknown`,
  `declaration-property-value-no-unknown`
- `at-mixin-no-risky-nesting-selector`, `block-no-redundant-nesting`,
  `at-root-no-redundant`
- `at-each-key-value-single-line`, `at-function-named-arguments`,
  `at-mixin-named-arguments`, `at-import-partial-extension-allowed-list`
- `dimension-no-non-numeric-values`, `function-calculation-no-interpolation`,
  `map-keys-quotes`, `media-feature-value-dollar-variable`
- `dollar-variable-default`, `dollar-variable-first-in-block`,
  `dollar-variable-colon-newline-after`, `dollar-variable-empty-line-after`,
  `no-dollar-variables`
- `selector-class-pattern`, `selector-nest-combinators`,
  `selector-no-union-class-name`

### C. Migration blockers

Expected before people switch; none of them is a reason to switch on its own.

| # | Feature | Why |
|--:|---------|-----|
| 18 | Stylelint v17 message wording | The README says most messages still use v15/v16 phrasing, which breaks snapshot tests and custom reporters |
| 19 | Stylelint v18 behavior, including `value-no-invalid` and `declaration-property-custom-property-allowed-list` | 17.16.0 is likely the last 17.x release |
| 20 | Run JavaScript configs, or at least more of their common patterns | GOV.UK Frontend fails with exit 78 in the README's benchmark table; PostHog uses `.stylelintrc.js` |
| 21 | Edit ranges in results (`computeEditInfo`) | Lets editors and agents apply fixes without rewriting files |
| 22 | `--custom-formatter` | Load a formatter from a JavaScript module |
| 23 | CSS-in-JS (styled-components and similar) | Matched files are skipped today |
| 24 | CSS code blocks in Markdown files | Matched files are skipped today |
| 25 | `languageOptions` | Declare extra at-rules, properties and types |
| 26 | Populate the Node API fields that are always empty: `deprecations`, `invalidOptionWarnings` beyond regular expressions, `ruleMetadata` | Tools built on `stylelint.lint()` read them |
| 27 | Smaller CLI flags: `--config-basedir`, `--globby-options`, `--validate` / `--no-validate` | Parity |

### D. Stylelint's long-standing open requests

Gale could ship these before Stylelint does. Reaction counts are from
Stylelint's issue tracker on 2026-10-09.

| # | Feature | Stylelint issue |
|--:|---------|-----------------|
| 28 | `files: []` in config, flat-config style | [#7408](https://github.com/stylelint/stylelint/issues/7408) (19 reactions), [#3860](https://github.com/stylelint/stylelint/issues/3860) (8) |
| 29 | JSONC config files | [#5139](https://github.com/stylelint/stylelint/issues/5139) (7) |
| 30 | `ignorePath` in the config file, plus shared `ignoreFiles` | [#8345](https://github.com/stylelint/stylelint/issues/8345) (8), [#6913](https://github.com/stylelint/stylelint/issues/6913) (3) |
| 31 | Opt-in fix for `no-duplicate-selectors` false positives with nesting (opt-in because it breaks exact parity) | [#7893](https://github.com/stylelint/stylelint/issues/7893) (19) |
| 32 | Clearer `no-descending-specificity` messages for nested rules | [#7844](https://github.com/stylelint/stylelint/issues/7844) (9) |
| 33 | `junit` formatter | [#7761](https://github.com/stylelint/stylelint/issues/7761) (3) |
| 34 | Exit codes per severity | [#7659](https://github.com/stylelint/stylelint/issues/7659) (2) |
| 35 | New rule against unconstrained `:has()`, for performance | [#7749](https://github.com/stylelint/stylelint/issues/7749) (3) |
| 36 | New rule requiring `prefers-reduced-motion` with animations and transitions | [#6202](https://github.com/stylelint/stylelint/issues/6202) (2) |
| 37 | New `max-lines` rule | [#3808](https://github.com/stylelint/stylelint/issues/3808) (2) |

## Beyond features

With 0 stars, nobody has heard of Gale yet. A Show HN or r/css launch built
around the agent guardrail mode and the no-effect rules would probably do more
than any single item above.

## Sources

- Reddit:
  [Why is Claude Code so bad at writing CSS?](https://reddit.com/comments/1v28uj9),
  [NoEffect](https://reddit.com/comments/1vuhw0b),
  [Is SCSS still worth learning in 2026](https://reddit.com/comments/1wpgmoc),
  [r/Frontend comment on agent linting](https://reddit.com/comments/1u2z984/_/or1bwfn)
- Hacker News:
  [Baseline comment](https://news.ycombinator.com/item?id=49267909),
  [design-token comment](https://news.ycombinator.com/item?id=48812271),
  [PostHog repo listing](https://news.ycombinator.com/item?id=48848915)
- Stylelint:
  [repository](https://github.com/stylelint/stylelint),
  [changelog](https://github.com/stylelint/stylelint/blob/main/CHANGELOG.md),
  [v18 preparation](https://github.com/stylelint/stylelint/issues/9338)
- [stylelint-scss rules](https://github.com/stylelint-scss/stylelint-scss/tree/master/src/rules)
- Sass: [`@import` is deprecated](https://sass-lang.com/blog/import-is-deprecated/),
  [dart-sass releases](https://github.com/sass/dart-sass/releases)
- Biome: [language support](https://biomejs.dev/internals/language-support/),
  [2026 roadmap](https://biomejs.dev/blog/roadmap-2026/),
  [SCSS tracking issue](https://github.com/biomejs/biome/issues/8732)
- [csskit](https://github.com/43081j/csskit)
- YouTube: [Linting & Code Quality Full Course (2026)](https://www.youtube.com/watch?v=LssrCwvk6TQ)
