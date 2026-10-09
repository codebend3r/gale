# r/css draft

A draft for you to edit and post yourself, written in your voice. r/css
allows self-promotion when you say plainly that you built the thing and the
post is useful on its own, so lead with the problem and the example, not the
project.

## Title

I made a linter rule that catches CSS declarations that do nothing (e.g. justify-content without flex or grid)

## Body

I kept finding CSS like this in reviews, especially in code written by AI
agents:

```css
.card {
  display: block;
  justify-content: center; /* does nothing: not a flex or grid container */
  position: static;
  top: 8px;                /* does nothing: offsets need a positioned box */
}
```

So I added a rule for it to Gale, a CSS/SCSS linter I've been building. It
reports, among others:

- flex and grid container properties (`justify-content`, `align-items`,
  `flex-wrap`, `grid-template-*`, `gap`) when the block's `display` is
  neither flex nor grid;
- `top`/`left`/`inset*` with `position: static`;
- `float` with `position: absolute` or `fixed`;
- `vertical-align` on a block-level box, and `table-layout` off a table.

It only reports what the block itself proves. A `justify-content` with no
`display` in the same block is left alone, because another rule may make it
a flex container. So are values from variables, blocks that include a mixin,
and resets like `top: auto`.

Gale reads your existing Stylelint config, so you can try the rule without
touching it. Add a `gale.json` next to your Stylelint config (Gale reads it
first; Stylelint never reads it, so it keeps working):

```json
{
  "extends": "./.stylelintrc.json",
  "rules": { "gale/no-ineffective-declarations": true }
}
```

```bash
npm install -D @codebend3r/gale
npx gale "src/**/*.{css,scss}"
```

It also has a strict preset and an output format meant for coding agents
that lint after every edit, if that is your situation:
https://github.com/codebend3r/gale/blob/main/docs/agents.md

Full disclosure: I built it. I'd love to hear which other "has no effect"
cases you run into.

## Before posting

- Publish a release to npm that includes `gale/no-ineffective-declarations`
  first; the post tells readers to install it.
- Reply to comments for the first few hours, and take false-positive
  reports seriously: the rule's promise is that it only reports what the
  block proves.
