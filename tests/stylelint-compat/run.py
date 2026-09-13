#!/usr/bin/env python3
"""
Stylelint compatibility test runner for Gale.

Reads test-cases.json (produced by extract.mjs) and runs Gale against each
test case to measure rule-level compatibility with Stylelint's own test suite.

Usage:
    python run.py                              # Run all tests
    python run.py --rule color-no-invalid-hex  # Run specific rule
    python run.py --source stylelint-scss      # Run specific source
    python run.py --failing-only               # Show only failures
    python run.py --skip-build                 # Skip building Gale
    python run.py --verbose                    # Show every case result
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from collections import defaultdict
from pathlib import Path

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

SCRIPT_DIR = Path(__file__).parent
TEST_CASES_FILE = SCRIPT_DIR / "test-cases.json"
RESULTS_DIR = SCRIPT_DIR / "results"
GALE_ROOT = SCRIPT_DIR.parent.parent  # gale/

# Rules Gale actually implements (pulled from the registry).
# We dynamically detect these by running `gale --print-config` or by
# maintaining this set. For now, we query the binary at startup.
_GALE_RULES_CACHE: set[str] | None = None

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def run_cmd(
    cmd: list[str],
    cwd: str | None = None,
    timeout: int = 60,
    stdin_data: str | None = None,
) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=timeout,
        input=stdin_data,
    )


def build_gale(skip: bool = False) -> Path | None:
    binary = GALE_ROOT / "target" / "release" / "gale"

    if skip:
        if not binary.exists():
            print("[error] No Gale binary found. Build first or remove --skip-build.")
            return None
        print(f"[build] Using existing binary: {binary}")
        return binary

    print("[build] Building Gale (release)...")
    result = run_cmd(
        ["cargo", "build", "--release"], cwd=str(GALE_ROOT), timeout=300
    )
    if result.returncode != 0:
        print(f"[error] Build failed: {result.stderr.strip()[:500]}")
        return None

    if not binary.exists():
        print("[error] Binary not found after build")
        return None

    print("[build] Gale binary ready")
    return binary


def get_gale_rules(gale_bin: Path) -> set[str]:
    """Discover which rules Gale supports by running it with a known config."""
    global _GALE_RULES_CACHE
    if _GALE_RULES_CACHE is not None:
        return _GALE_RULES_CACHE

    # Create a temp config that extends gale:all
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", prefix="gale_all_", delete=False
    ) as f:
        json.dump({"extends": "gale:all"}, f)
        config_path = f.name

    try:
        result = run_cmd(
            [str(gale_bin), "--print-config", "test.css", "--config", config_path],
            timeout=10,
        )
        if result.returncode == 0 and result.stdout.strip():
            config = json.loads(result.stdout.strip())
            rules = set(config.get("rules", {}).keys())
            _GALE_RULES_CACHE = rules
            return rules
    except (json.JSONDecodeError, subprocess.TimeoutExpired):
        pass
    finally:
        os.unlink(config_path)

    # Fallback: hardcoded set from CLAUDE.md
    print("[warn] Could not detect Gale rules dynamically, using hardcoded list")
    _GALE_RULES_CACHE = {
        "alpha-value-notation", "annotation-no-unknown", "at-rule-no-unknown",
        "at-rule-no-vendor-prefix", "block-no-empty", "color-hex-case",
        "color-hex-length", "color-named", "color-no-invalid-hex",
        "comment-empty-line-before", "comment-no-empty",
        "custom-property-no-missing-var-function", "custom-property-pattern",
        "declaration-block-no-duplicate-custom-properties",
        "declaration-block-no-duplicate-properties",
        "declaration-block-no-redundant-longhand-properties",
        "declaration-block-no-shorthand-property-overrides",
        "declaration-empty-line-before", "declaration-no-important",
        "font-family-no-duplicate-names",
        "font-family-no-missing-generic-family-keyword",
        "function-calc-no-unspaced-operator", "function-name-case",
        "function-url-quotes", "import-notation",
        "keyframe-block-no-duplicate-selectors",
        "keyframe-declaration-no-important", "length-zero-no-unit",
        "max-nesting-depth", "media-feature-name-no-unknown",
        "media-query-no-invalid", "no-descending-specificity",
        "no-duplicate-at-import-rules", "no-duplicate-selectors",
        "no-empty-source", "no-invalid-double-slash-comments",
        "no-invalid-position-at-import-rule", "no-invalid-position-declaration",
        "no-irregular-whitespace", "no-unknown-animations",
        "number-max-precision", "property-no-unknown", "property-no-vendor-prefix",
        "rule-empty-line-before", "selector-class-pattern",
        "selector-max-compound-selectors", "selector-max-id",
        "selector-no-qualifying-type", "selector-pseudo-class-no-unknown",
        "selector-pseudo-element-colon-notation",
        "selector-pseudo-element-no-unknown", "selector-type-no-unknown",
        "shorthand-property-no-redundant-values", "string-no-newline",
        "unit-no-unknown", "value-keyword-case", "value-no-vendor-prefix",
    }
    return _GALE_RULES_CACHE


# ---------------------------------------------------------------------------
# Test runner
# ---------------------------------------------------------------------------

# File extension mapping
SYNTAX_EXT = {
    "css": ".css",
    "scss": ".scss",
    "less": ".less",
}


def make_config(rule_name: str, config_value) -> dict:
    """Create a minimal Gale config enabling only the given rule."""
    # Normalize config value to Stylelint format
    if config_value is True or config_value is None:
        rule_config = True
    elif config_value is False:
        rule_config = False
    elif isinstance(config_value, str):
        rule_config = config_value
    elif isinstance(config_value, list):
        # [primary, secondaryOptions] -> Gale expects the same
        rule_config = config_value
    else:
        rule_config = config_value

    return {"rules": {rule_name: rule_config}}


def run_gale_batch(
    gale_bin: Path,
    cases: list[dict],
    rule_name: str,
    config_value,
    syntax: str,
) -> list[dict]:
    """
    Run Gale against a batch of test cases for a single rule.

    Returns a list of dicts: { "index": int, "warnings": [...] }
    for each case in the batch.
    """
    ext = SYNTAX_EXT.get(syntax, ".css")
    config = make_config(rule_name, config_value)
    results = []

    # Create a temp directory for this batch
    with tempfile.TemporaryDirectory(prefix="gale_compat_") as tmpdir:
        tmpdir_path = Path(tmpdir)

        # Write config
        config_path = tmpdir_path / ".stylelintrc.json"
        with open(config_path, "w") as f:
            json.dump(config, f)

        # Write all case files
        file_paths = []
        for i, case in enumerate(cases):
            file_name = f"case_{i:04d}{ext}"
            file_path = tmpdir_path / file_name
            with open(file_path, "w") as f:
                f.write(case["code"])
            file_paths.append(file_path)

        # Run Gale on all files at once (batching for performance)
        batch_size = 100
        all_gale_results = []

        for batch_start in range(0, len(file_paths), batch_size):
            batch_files = file_paths[batch_start : batch_start + batch_size]
            cmd = [
                str(gale_bin),
                "--formatter", "json",
                "--config", str(config_path),
            ] + [str(f) for f in batch_files]

            try:
                result = run_cmd(cmd, cwd=str(tmpdir_path), timeout=30)
            except subprocess.TimeoutExpired:
                # If timeout, return empty results for this batch
                for j in range(len(batch_files)):
                    all_gale_results.append({"index": batch_start + j, "warnings": [], "error": "timeout"})
                continue

            # Parse JSON output
            stdout = result.stdout.strip()
            if not stdout:
                for j in range(len(batch_files)):
                    all_gale_results.append({"index": batch_start + j, "warnings": []})
                continue

            try:
                gale_output = json.loads(stdout)
            except json.JSONDecodeError:
                for j in range(len(batch_files)):
                    all_gale_results.append({"index": batch_start + j, "warnings": [], "error": "json_parse"})
                continue

            # Map results back to case indices
            file_to_index = {}
            for j, fp in enumerate(batch_files):
                file_to_index[str(fp)] = batch_start + j

            found_indices = set()
            for entry in gale_output:
                source = entry.get("source", "")
                warnings = entry.get("warnings", [])
                # Filter to only warnings from the target rule
                rule_warnings = [
                    w for w in warnings if w.get("rule") == rule_name
                ]

                idx = file_to_index.get(source)
                if idx is not None:
                    all_gale_results.append({"index": idx, "warnings": rule_warnings})
                    found_indices.add(idx)

            # Any files not in output had 0 warnings
            for j in range(len(batch_files)):
                global_idx = batch_start + j
                if global_idx not in found_indices:
                    all_gale_results.append({"index": global_idx, "warnings": []})

        # Sort by index
        all_gale_results.sort(key=lambda r: r["index"])
        return all_gale_results


def check_case(case: dict, gale_result: dict) -> dict:
    """
    Check a single test case against Gale's output.

    Returns: { "passed": bool, "reason": str }
    """
    warnings = gale_result.get("warnings", [])
    error = gale_result.get("error")

    if error:
        return {"passed": False, "reason": f"Gale error: {error}"}

    if case["type"] == "accept":
        if len(warnings) == 0:
            return {"passed": True, "reason": ""}
        return {
            "passed": False,
            "reason": f"Expected 0 warnings, got {len(warnings)}: {warnings[0].get('text', '')[:80]}",
        }

    elif case["type"] == "reject":
        if len(warnings) == 0:
            return {"passed": False, "reason": "Expected >= 1 warning, got 0"}

        result = {"passed": True, "reason": ""}

        # Optionally check line number
        if "line" in case and case["line"] is not None:
            expected_line = case["line"]
            actual_line = warnings[0].get("line")
            if actual_line != expected_line:
                result = {
                    "passed": False,
                    "reason": f"Line mismatch: expected {expected_line}, got {actual_line}",
                }

        # Optionally check column number
        if result["passed"] and "column" in case and case["column"] is not None:
            expected_col = case["column"]
            actual_col = warnings[0].get("column")
            if actual_col != expected_col:
                result = {
                    "passed": False,
                    "reason": f"Column mismatch: expected {expected_col}, got {actual_col}",
                }

        return result

    return {"passed": False, "reason": f"Unknown case type: {case['type']}"}


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------


def print_source_report(source_name: str, rule_results: dict, failing_only: bool = False):
    """Print report for a single source (stylelint, stylelint-scss, stylelint-order)."""
    total_rules = len(rule_results)
    if total_rules == 0:
        return

    total_cases = 0
    total_passing = 0
    total_failing = 0
    rule_summaries = []

    for rule_name, results in sorted(rule_results.items()):
        passing = sum(1 for r in results if r["passed"])
        failing = sum(1 for r in results if not r["passed"])
        total_cases += len(results)
        total_passing += passing
        total_failing += failing
        rule_summaries.append({
            "rule": rule_name,
            "total": len(results),
            "passing": passing,
            "failing": failing,
            "failures": [r for r in results if not r["passed"]],
        })

    label_map = {
        "stylelint": "Stylelint Core",
        "stylelint-scss": "SCSS Plugin",
        "stylelint-order": "Order Plugin",
    }
    label = label_map.get(source_name, source_name)

    print(f"\n{label} Compatibility")
    print("=" * 60)
    print(f"  Rules tested:        {total_rules}")
    print(f"  Test cases:          {total_cases:,}")
    if total_cases > 0:
        pct = total_passing / total_cases * 100
        print(f"  Passing:             {total_passing:,} ({pct:.1f}%)")
        print(f"  Failing:             {total_failing:,} ({100 - pct:.1f}%)")

    # Top failures (rules with most failing cases)
    failing_rules = [s for s in rule_summaries if s["failing"] > 0]
    failing_rules.sort(key=lambda s: s["failing"], reverse=True)

    if failing_rules:
        print(f"\n  {'Rule':<50} {'Pass Rate':<15}")
        print(f"  {'-' * 65}")

        display_rules = failing_rules if failing_only else rule_summaries
        if failing_only:
            display_rules = failing_rules
        else:
            display_rules = sorted(rule_summaries, key=lambda s: s["rule"])

        for s in display_rules:
            if failing_only and s["failing"] == 0:
                continue
            rate = f"{s['passing']}/{s['total']}"
            status = "PASS" if s["failing"] == 0 else f"{s['failing']} fails"
            print(f"  {s['rule']:<50} {rate:<10} {status}")

    print()


def print_failing_details(rule_results_by_source: dict, max_per_rule: int = 5):
    """Print detailed failure information."""
    print("\nDetailed Failures")
    print("=" * 60)

    for source, rule_results in sorted(rule_results_by_source.items()):
        for rule_name, results in sorted(rule_results.items()):
            failures = [r for r in results if not r["passed"]]
            if not failures:
                continue

            print(f"\n  {rule_name} ({source}):")
            for i, f in enumerate(failures[:max_per_rule]):
                case_type = f["case"]["type"]
                code = f["case"]["code"]
                # Truncate long code
                if len(code) > 80:
                    code = code[:77] + "..."
                code = code.replace("\n", "\\n")
                print(f"    [{case_type}] {code}")
                print(f"           {f['reason']}")

            if len(failures) > max_per_rule:
                print(f"    ... and {len(failures) - max_per_rule} more failures")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main():
    parser = argparse.ArgumentParser(
        description="Stylelint compatibility test runner for Gale"
    )
    parser.add_argument("--rule", type=str, help="Test only this rule")
    parser.add_argument(
        "--source",
        type=str,
        choices=["stylelint", "stylelint-scss", "stylelint-order"],
        help="Test only this source",
    )
    parser.add_argument(
        "--failing-only", action="store_true", help="Show only failing rules"
    )
    parser.add_argument(
        "--skip-build", action="store_true", help="Skip building Gale"
    )
    parser.add_argument(
        "--gale-bin", type=str, help="Path to pre-built Gale binary"
    )
    parser.add_argument(
        "--verbose", action="store_true", help="Show every case result"
    )
    parser.add_argument(
        "--details", action="store_true", help="Show detailed failure info"
    )
    args = parser.parse_args()

    # Load test cases
    if not TEST_CASES_FILE.exists():
        print(f"[error] {TEST_CASES_FILE} not found.")
        print("Run 'node extract.mjs' first to extract test cases.")
        sys.exit(1)

    with open(TEST_CASES_FILE) as f:
        test_groups = json.load(f)

    print(f"[load] Loaded {len(test_groups)} test groups from {TEST_CASES_FILE.name}")

    # Build or locate Gale
    if args.gale_bin:
        gale_bin = Path(args.gale_bin)
        if not gale_bin.exists():
            print(f"[error] Binary not found: {gale_bin}")
            sys.exit(1)
    else:
        gale_bin = build_gale(skip=args.skip_build)
        if gale_bin is None:
            sys.exit(1)

    # Get supported rules
    gale_rules = get_gale_rules(gale_bin)
    print(f"[rules] Gale supports {len(gale_rules)} rules")

    # Filter test groups
    filtered = test_groups
    if args.source:
        filtered = [g for g in filtered if g["source"] == args.source]
    if args.rule:
        filtered = [g for g in filtered if g["rule"] == args.rule]

    # Filter to only rules Gale implements
    supported = [g for g in filtered if g["rule"] in gale_rules]
    skipped_rules = set(g["rule"] for g in filtered) - gale_rules
    if skipped_rules:
        print(
            f"[filter] Skipping {len(skipped_rules)} rules not implemented in Gale"
        )
        if args.verbose:
            for r in sorted(skipped_rules):
                print(f"  - {r}")

    total_groups = len(supported)
    total_cases = sum(len(g["cases"]) for g in supported)
    print(
        f"[test] Running {total_cases:,} test cases across {total_groups} groups\n"
    )

    if total_groups == 0:
        print("[warn] No test groups to run.")
        sys.exit(0)

    # Run tests
    # Group by (source, rule, config, syntax) for batching
    rule_results_by_source: dict[str, dict[str, list]] = defaultdict(
        lambda: defaultdict(list)
    )

    t_start = time.time()
    processed = 0

    for group in supported:
        source = group["source"]
        rule = group["rule"]
        config = group["config"]
        syntax = group["syntax"]
        cases = group["cases"]

        if not cases:
            continue

        # Run Gale on this batch
        gale_results = run_gale_batch(gale_bin, cases, rule, config, syntax)

        # Check each case
        for i, case in enumerate(cases):
            # Find matching gale result
            gale_result = next(
                (r for r in gale_results if r["index"] == i),
                {"index": i, "warnings": []},
            )

            check = check_case(case, gale_result)
            entry = {
                "passed": check["passed"],
                "reason": check["reason"],
                "case": case,
                "config": config,
                "syntax": syntax,
            }
            rule_results_by_source[source][rule].append(entry)

            if args.verbose:
                status = "PASS" if check["passed"] else "FAIL"
                code_preview = case["code"][:60].replace("\n", "\\n")
                print(f"  [{status}] {rule} [{case['type']}] {code_preview}")
                if not check["passed"]:
                    print(f"         {check['reason']}")

        processed += 1
        # Progress indicator every 20 groups
        if processed % 20 == 0:
            elapsed = time.time() - t_start
            print(f"  ... processed {processed}/{total_groups} groups ({elapsed:.1f}s)")

    elapsed = time.time() - t_start
    print(f"\n[done] Completed in {elapsed:.1f}s")

    # Print reports per source
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)

    # Overall stats
    grand_total = 0
    grand_passing = 0

    for source in ["stylelint", "stylelint-scss", "stylelint-order"]:
        if source in rule_results_by_source:
            print_source_report(
                source, rule_results_by_source[source], failing_only=args.failing_only
            )
            for results in rule_results_by_source[source].values():
                grand_total += len(results)
                grand_passing += sum(1 for r in results if r["passed"])

    # Overall summary
    if grand_total > 0:
        pct = grand_passing / grand_total * 100
        print("=" * 60)
        print(f"Overall: {grand_passing:,}/{grand_total:,} ({pct:.1f}%) passing")
        print("=" * 60)

    # Detailed failures
    if args.details or args.failing_only:
        print_failing_details(rule_results_by_source)

    # Save results JSON
    results_data = {}
    for source, rule_results in rule_results_by_source.items():
        results_data[source] = {}
        for rule, results in rule_results.items():
            results_data[source][rule] = {
                "total": len(results),
                "passing": sum(1 for r in results if r["passed"]),
                "failing": sum(1 for r in results if not r["passed"]),
            }

    results_file = RESULTS_DIR / "compat-results.json"
    with open(results_file, "w") as f:
        json.dump(results_data, f, indent=2)
    print(f"\n[save] Results saved to {results_file}")


if __name__ == "__main__":
    main()
