# Gale

**An extremely fast CSS linter. Drop-in replacement for Stylelint.**

100x-400x faster. Same config. Zero migration.

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

See the full documentation at [github.com/LyricalString/gale](https://github.com/LyricalString/gale).
