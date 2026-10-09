# Using Gale with coding agents

Coding agents write CSS that works but drifts: raw hex colours, magic-number
margins, `!important`, ID selectors, selectors nested six levels deep, and
declarations that do nothing.
Instructions in a `CLAUDE.md` or rules file help until the session gets long
and they fall out of context. A linter does not. When the agent runs it after
every edit, each violation lands back in the agent's loop and the agent fixes
it.

That only works if the linter is fast enough to run after every edit. Gale
lints most projects in tens of milliseconds, where Stylelint takes seconds.

Three pieces make this work:

- the `gale:strict` preset, which turns on the rules that catch agent drift;
- the `agent` formatter, which prints problems in a shape an agent acts on;
- a hook that runs Gale after the agent edits a style file.

## The `gale:strict` preset

`gale:strict` is `gale:recommended` plus these rules, all at error severity so
a violation fails the run:

| Rule | Setting | Catches |
|------|---------|---------|
| `plugin/enforce-variable-for-property` | Colours and spacing must come from a variable | `color: #333`, `margin: 16px` |
| `declaration-no-important` | On | `!important` |
| `selector-max-id` | `0` | `#app .title` |
| `max-nesting-depth` | `3` | Nesting deeper than three levels |
| `color-named` | `"never"` | `color: red` |
| `gale/no-ineffective-declarations` | On | `justify-content` next to `display: block`, `top` with `position: static` |

A value counts as coming from a variable when it uses `var()`, a Sass
variable or module member (`$text`, `tokens.$text`, `math.div($space, 2)`) or
a Less variable (`@text`). CSS-wide keywords (`inherit`, `initial`, `unset`,
`revert`, `revert-layer`) are always allowed, as are `currentcolor`,
`transparent` and `none` for colours and `0` and `auto` for spacing.

The colour rule covers `color`, `background-color`, every `border-*-color`,
`outline-color`, `text-decoration-color`, `text-emphasis-color`,
`caret-color`, `accent-color`, `column-rule-color`, `fill`, `stroke`,
`stop-color`, `flood-color` and `lighting-color`. The spacing rule covers
every `margin-*` and `padding-*` property, `gap`, `row-gap` and `column-gap`.
Custom property definitions (`--brand: #0af`) are never checked, so tokens
can be defined anywhere.

Stylelint cannot resolve `gale:strict`, so if anything else still runs
Stylelint on your project (an editor extension, another CI job), keep your
Stylelint config as it is and add a `gale.json` next to it that extends it.
Gale reads `gale.json` before any Stylelint config file, and Stylelint never
reads it. Later entries in `extends` win, and your own `rules` win over both:

```json
{
  "extends": ["./.stylelintrc.json", "gale:strict"],
  "rules": {
    "max-nesting-depth": [4, { "severity": "error" }]
  }
}
```

If Gale is the only CSS linter in the project, add `gale:strict` to `extends`
in your existing config instead.

To allow more values for colours or spacing, set
`plugin/enforce-variable-for-property` yourself. Your setting replaces the
preset's, so list every property you want checked. See the rule's options
in the [README](../README.md#declarative-plugin-rules).

## The `agent` formatter

```bash
gale --fix --formatter agent "src/**/*.{css,scss}"
```

```text
src/card.css:2:3: error: Expected variable or allowed value for 'color', got '#333' [plugin/enforce-variable-for-property]
src/card.css:9:12: warning: Expected "#ffffff" to be "#fff" [color-hex-length, fixable]
2 problems (1 error, 1 warning), 1 fixable with `gale --fix`
```

- One line per problem, compiler style: path relative to the working
  directory, line, column, severity, message, then the rule in brackets.
- `fixable` marks a problem `gale --fix` can fix, so the agent runs the fix
  instead of editing by hand.
- One summary line at the end.
- A clean run prints nothing.

Run it with `--fix` so Gale fixes what it can first and the agent only sees
what is left. The formatter is a command-line option; `formatters` in the
Node API does not include it.

## Claude Code

Claude Code runs a [hook](https://docs.claude.com/en/docs/claude-code/hooks)
after each tool call. When a `PostToolUse` hook exits with code 2, Claude sees
what it wrote to stderr and fixes it.

Save this as `.claude/hooks/gale.sh` and make it executable
(`chmod +x .claude/hooks/gale.sh`). It needs [`jq`](https://jqlang.org/).

```sh
#!/bin/sh
# Lint the style file Claude just edited, and hand what is left back to Claude.
file=$(jq -r '.tool_input.file_path // empty')
case "$file" in
  *.css | *.scss | *.sass | *.less | *.vue | *.svelte | *.astro | *.html) ;;
  *) exit 0 ;;
esac

cd "$CLAUDE_PROJECT_DIR" || exit 0
if report=$(./node_modules/.bin/gale --fix --allow-empty-input --formatter agent "$file"); then
  exit 0
fi
echo "$report" >&2
exit 2
```

Then register it in `.claude/settings.json`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write",
        "hooks": [{ "type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/gale.sh" }]
      }
    ]
  }
}
```

Gale exits 2 when an error-severity problem is left, which is when the hook
reports back. Warnings alone exit 0 and stay quiet; with `gale:strict` every
guardrail rule is an error. `--allow-empty-input` keeps a file your ignore
files exclude from counting as a failure.

## Other agents

For Cursor, Codex and other agents, add the command to the instructions the
agent reads (`AGENTS.md`, `.cursor/rules`, and so on):

```markdown
After editing any CSS, SCSS, Less, Vue, Svelte or Astro file, run
`npx gale --fix --formatter agent <the files you changed>` and fix every
problem it prints before moving on. No output means there is nothing to fix.
```

The agent formatter is built for this: the agent gets the file, line, rule
and whether `--fix` handles it, and nothing else.
