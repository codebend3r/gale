# Stylelint Compatibility Test Harness

Extracts `testRule()` cases from Stylelint's official repos and runs them against Gale to measure rule-level compatibility, both for the warnings a rule reports and for the output of its autofix.

## Prerequisites

- Bun for `extract.mjs`
- Python 3.10+ for `run.py`
- Git (for cloning repos)

## Usage

All commands run from `tests/stylelint-compat/`.

```bash
# Step 1: Extract test cases from Stylelint repos (once per checkout)
bun extract.mjs                            # Clone the repos into .clones/ and extract
bun extract.mjs --no-clone                 # Re-extract from the existing clones
bun extract.mjs --update                   # Pull the latest upstream first
bun extract.mjs --verbose                  # Log every skipped group and why

# Step 2: Run Gale against extracted cases
python run.py                              # Run all tests
python run.py --rule color-no-invalid-hex  # Run specific rule
python run.py --source stylelint-scss      # Run specific source
python run.py --failing-only               # Show only failures
python run.py --skip-build                 # Skip building Gale
```

## Fix mode

`--fix` checks autofix output instead of warnings. For every reject case that Stylelint verifies with `fix: true`, it writes the code to a temp file with the syntax's extension, runs `gale --fix` with a config enabling only that rule (with the case's options), and compares the file afterwards to the case's `fixed` byte for byte. Cases marked `unfixable` must come back unchanged.

Each rule gets a status:

| Status | Meaning |
| --- | --- |
| `PASS` | Every fix case matches |
| `partial` | Some `fixed` cases match |
| `wrong output` | Gale changes the code, but never to Stylelint's output |
| `no fix` | Gale leaves every `fixed` case unchanged |

Each failure says why: Gale did not report the problem, reported it but did not fix it, hit a parse error, rejected the options, or produced different output. The exit code is 1 when any selected fix case fails.

```bash
python run.py --fix                                        # Every rule with fix cases
python run.py --fix --rule alpha-value-notation            # One rule, with every failure listed
python run.py --fix --rule hue-degree-notation,lightness-notation  # Comma-separated rules
python run.py --fix --rule order/order --rule order/properties-order  # Repeated --rule
python run.py --fix --source stylelint-order               # One source
python run.py --fix --failing-only                         # Only rules that are not passing
python run.py --fix --verbose                              # Every case, and a unified diff per failure
python run.py --fix --skip-build                           # Use target/release/gale as it is
python run.py --fix --binary ../gale-other/target/release/gale  # Another worktree's build (no cargo)
python run.py --fix --cases /path/to/test-cases.json       # Test cases extracted elsewhere
python run.py --fix --json fix-report.json                 # Also write a machine-readable report
python run.py --fix --timeout 120                          # Seconds allowed per Gale run (default 60)
```

`--binary` (alias `--gale-bin`), `--cases` and `--json` work in the default warning mode too.

In the unified diffs, `\r` and `\t` are spelled out and trailing spaces show as `·`, so whitespace-only differences stay visible.

### Checking one rule from another worktree

`test-cases.json` and `.clones/` are gitignored, so a fresh worktree has neither. Either extract once in that worktree (`bun extract.mjs`, a few seconds), or point `--cases` at a checkout that already has them:

```bash
python3 tests/stylelint-compat/run.py --fix --rule alpha-value-notation \
  --binary target/release/gale --cases /path/to/other/tests/stylelint-compat/test-cases.json
```

### JSON report

`--json PATH` writes:

```json
{
  "mode": "fix",
  "binary": "/abs/path/to/gale",
  "summary": { "rules": 60, "cases": 1704, "passed": 362, "failed": 1342,
               "byStatus": { "pass": 1, "partial": 8, "fail": 0, "no-fix": 51 } },
  "rules": [
    { "rule": "alpha-value-notation", "source": "stylelint", "status": "no-fix",
      "total": 28, "passed": 0, "fixedTotal": 28, "fixedPassed": 0,
      "unfixableTotal": 0, "unfixablePassed": 0, "changed": 0,
      "failures": [
        { "file": "lib/rules/alpha-value-notation/__tests__/index.mjs", "sourceLine": 98,
          "kind": "fixed", "config": ["number"], "syntax": "css",
          "code": "a { opacity: 10% }", "expected": "a { opacity: 0.1 }",
          "actual": "a { opacity: 10% }", "reason": "gale reported the problem but did not fix it" }
      ] }
  ],
  "notImplemented": [{ "rule": "selector-no-deprecated", "source": "stylelint", "cases": 16 }]
}
```

`changed` counts the `fixed` cases whose code Gale altered at all. `notImplemented` lists rules with fix cases that Gale does not have.

## How it works

1. **extract.mjs** shallow-clones three repos (`stylelint/stylelint`, `stylelint-scss/stylelint-scss`, `hudochenkov/stylelint-order`) into `.clones/` and reads their test files to extract `testRule()` blocks into `test-cases.json`. The files are never executed: a small static evaluator handles the JavaScript the suites use for test data (string and template literals with exact escapes, the `stripIndent` and `dedent` tags, `+` concatenation, top-level `const` references, `...spread`, `.concat()` and `mergeTestDescriptions()`). Blocks it cannot read statically are skipped and counted in the summary, as are groups for syntaxes Gale does not parse (HTML, CSS-in-JS, SugarSS) and cases Stylelint itself marks `skip`.

2. **run.py** reads `test-cases.json`, filters to only rules Gale implements, and for each test case:
   - Creates a temp CSS/SCSS/Less/Sass file
   - Creates a temp config enabling only that rule
   - Runs Gale with `--formatter json`
   - Checks accept cases produce 0 warnings and reject cases produce >= 1 warning
   - Reports per-rule and per-source pass rates

   With `--fix` it runs `gale --fix` instead and compares the fixed files, as described above.

### Adding a repo

Each source is one entry in `REPOS` in `extract.mjs`: the GitHub repo and branch, a glob for its test files (the path segment that is exactly `*` names the rule), and a rule prefix. `@stylistic/stylelint-plugin` is not extracted yet: its suites run on `@morev/stylelint-testing-library` with `autoStripIndent: true`, so `code` and `fixed` need that library's indent stripping before they compare byte for byte. The comment in `REPOS` has the entry and the steps.

## Output format

`test-cases.json` contains an array of test groups:

```json
[
  {
    "source": "stylelint",
    "rule": "color-hex-length",
    "config": ["short"],
    "syntax": "css",
    "file": "lib/rules/color-hex-length/__tests__/index.mjs",
    "line": 20,
    "fix": true,
    "computeEditInfo": true,
    "cases": [
      {
        "type": "accept",
        "code": "a { border-#$side: 0; }",
        "description": "ignore sass-like interpolation",
        "sourceLine": 9
      },
      {
        "type": "reject",
        "code": "a { color: #FFFFFF; }",
        "sourceLine": 74,
        "line": 1,
        "column": 12,
        "endLine": 1,
        "endColumn": 19,
        "fixed": "a { color: #FFF; }",
        "fix": { "range": [15, 18], "text": "" }
      }
    ]
  }
]
```

- `fix` is the group's `fix: true` flag. Like jest-preset-stylelint, fix mode only checks reject cases in groups that set it.
- `fixed` is the expected output after autofix, and `unfixable: true` marks a reject case the fix must leave unchanged.
- `computeEditInfo` is copied from the group and from cases that set it, and a case's `fix` is the edit Stylelint expects to report for it.
- `file`, `line` and `sourceLine` point back at the upstream test file, so a failing case can be found quickly.
