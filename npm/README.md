# Gale

**An extremely fast CSS linter. Drop-in replacement for Stylelint.**

Typically 10-100x faster than Stylelint. Same config. Zero migration.

Fast enough to run after every edit, Gale also does what Stylelint does not:

- **Built for coding agents.** [`gale:strict` and the `agent` formatter](https://github.com/codebend3r/gale/blob/main/docs/agents.md) keep AI-written CSS in line: colours and spacing from variables, no `!important`, no ID selectors, and a report the agent acts on.
- **Catches CSS that does nothing.** `gale/no-ineffective-declarations` reports declarations the block switches off, like `justify-content` on a box that is neither flex nor grid, or `top` on a static one.
- **Ready for Dart Sass 3.** `gale:sass3` reports the `@import` rules and global functions Dart Sass 3 removes, and `--fix` rewrites `map-get()` to `map.get()` with the `@use` it needs.

> **Compatibility:** Gale targets **Stylelint v17** semantics.

```bash
npm install -D @codebend3r/gale

# Uses your existing .stylelintrc
npx gale "src/**/*.css"
```

## Programmatic API

```javascript
import { lint, resolveConfig, formatters } from '@codebend3r/gale';

const result = await lint({
  files: 'src/**/*.css',
  config: { rules: { 'block-no-empty': true } },
});

console.log(result.errored);
console.log(result.results);
```

See the full documentation at [github.com/codebend3r/gale](https://github.com/codebend3r/gale).
