# Stylelint Compatibility Test Harness

Extracts `testRule()` cases from Stylelint's official repos and runs them against Gale to measure rule-level compatibility.

## Prerequisites

- Node.js (or Bun) for `extract.mjs`
- Python 3.10+ for `run.py`
- Git (for cloning repos)

## Usage

```bash
# Step 1: Extract test cases from Stylelint repos
node extract.mjs              # or: bun run extract.mjs

# Step 2: Run Gale against extracted cases
python run.py                 # Run all tests
python run.py --rule color-no-invalid-hex  # Run specific rule
python run.py --source stylelint-scss      # Run specific source
python run.py --failing-only               # Show only failures
python run.py --skip-build                 # Skip building Gale
```

## How it works

1. **extract.mjs** shallow-clones three repos (`stylelint/stylelint`, `stylelint-scss/stylelint-scss`, `hudochenkov/stylelint-order`) and parses their test files to extract `testRule()` blocks into `test-cases.json`.

2. **run.py** reads `test-cases.json`, filters to only rules Gale implements, and for each test case:
   - Creates a temp CSS/SCSS/Less file
   - Creates a temp config enabling only that rule
   - Runs Gale with `--formatter json`
   - Checks accept cases produce 0 warnings and reject cases produce >= 1 warning
   - Reports per-rule and per-source pass rates

## Output format

`test-cases.json` contains an array of test groups:

```json
[
  {
    "source": "stylelint",
    "rule": "color-no-invalid-hex",
    "config": true,
    "syntax": "css",
    "cases": [
      { "type": "accept", "code": "a { color: pink; }" },
      { "type": "reject", "code": "a { color: #ababa; }", "line": 1, "column": 12 }
    ]
  }
]
```
