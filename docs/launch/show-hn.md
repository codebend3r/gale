# Show HN draft

A draft for you to edit and post yourself. It is written in your voice:
change anything that does not sound like you, and fill in the one bracketed
placeholder. Show HN posts do best when the first comment explains why you
built the thing, so the body below is meant to be that first comment, with
the link in the post itself.

## Title

Show HN: Gale – Rust CSS linter that runs your Stylelint config 10-100x faster

(HN allows 80 characters; this is 78.)

## URL

https://github.com/codebend3r/gale

## First comment

I built Gale because [YOUR REASON, in a sentence; for example, Stylelint was
the slowest step in our lint pipeline]. It reads your existing `.stylelintrc`
(JSON, YAML, or a statically parsed JS config), runs the same rules, and aims
for byte-for-byte identical warnings.
On wp-calypso's 2,051 style files it takes 0.46s where Stylelint takes 23.9s;
across 19 open-source repos the speedup ranges from 11x to 122x. SCSS, Less,
`@stylistic`, `stylelint-order` and Vue/Svelte/Astro `<style>` blocks are
built in, so there are no plugins to install.

Speed mattered more than I expected once coding agents started writing CSS.
Agents drift: raw hex colours, magic-number margins, `!important`, ID
selectors. Instructions in a CLAUDE.md fade as the session gets long; a lint
error in the agent's loop does not. But that only works if the linter can run
after every edit, which a 5-second lint cannot. So Gale has:

- `gale:strict`, a preset that requires colours and spacing to come from
  variables and rejects `!important`, IDs and deep nesting, all as errors;
- `--formatter agent`, one compiler-style line per problem with a `fixable`
  marker, and nothing at all on a clean run;
- a Claude Code hook recipe that lints each style file the agent edits and
  hands problems back to it.

Two things Stylelint does not have:

- `gale/no-ineffective-declarations` reports CSS that does nothing, such as
  `justify-content` next to `display: block`, or `top` on a static box. It
  only reports what the block itself proves, so it does not guess about the
  cascade.
- `gale:sass3` gets SCSS ready for Dart Sass 3, which removes `@import` and
  the global built-in functions. `--fix` rewrites `map-get()` to `map.get()`
  and adds the `@use "sass:map"` it needs.

What it does not do yet: run arbitrary JavaScript plugins (the common plugin
patterns are built in as declarative rules), execute JS configs that compute
rules at runtime, lint CSS-in-JS, or match Stylelint v17's reworded messages
exactly (positions and rule IDs match; some message text still uses v16
wording). The README has a feature-by-feature table against Stylelint.

Feedback on parity gaps is the most useful thing you can give me: if Gale
reports something different from Stylelint on your repo, that is a bug.

## Before posting

- Merge the feature PRs and publish a release to npm first: the post
  describes `gale:strict`, the agent formatter,
  `gale/no-ineffective-declarations` and `gale:sass3`, and readers will
  install from npm.
- Post on a weekday morning US time, and stay around for the first two hours
  to answer comments.
- Have the benchmark script ready to point at (`benchmarks/benchmark.sh`);
  someone will ask how the numbers were measured.
- Expect "why not Biome?". An honest answer: Biome is great for CSS, but it
  cannot lint SCSS yet and uses its own rule set; Gale runs the Stylelint
  config a team already has.
