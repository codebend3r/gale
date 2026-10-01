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

    python run.py --fix                        # Check autofix output instead
    python run.py --fix --rule a,b --rule c    # Several rules
    python run.py --fix --binary path/to/gale  # Use another build (no cargo)
    python run.py --fix --json out.json        # Also write a JSON report
"""

import argparse
import difflib
import json
import os
import subprocess
import sys
import tempfile
import time
from collections import defaultdict
from datetime import datetime, timezone
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
        ["cargo", "build", "--release"], cwd=str(GALE_ROOT), timeout=1800
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

    # Fallback: hardcoded list of the rules Gale implements
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
    "sass": ".sass",
}

SOURCE_ORDER = ["stylelint", "stylelint-scss", "stylelint-order"]
SOURCE_LABELS = {
    "stylelint": "Stylelint Core",
    "stylelint-scss": "SCSS Plugin",
    "stylelint-order": "Order Plugin",
}


def ordered_sources(sources) -> list[str]:
    """Known sources first, in their usual order, then any others."""
    sources = set(sources)
    return [s for s in SOURCE_ORDER if s in sources] + sorted(sources - set(SOURCE_ORDER))


def source_rank(source: str) -> int:
    return SOURCE_ORDER.index(source) if source in SOURCE_ORDER else len(SOURCE_ORDER)


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

    label = SOURCE_LABELS.get(source_name, source_name)

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
# Fix mode
# ---------------------------------------------------------------------------
#
# Mirrors jest-preset-stylelint: a reject case is checked for its autofix only
# when its testRule() block sets `fix: true`.  `fixed` is the exact expected
# output; `unfixable: true` means the fix must leave the code untouched.

FIX_STATUS_LABELS = {
    "pass": "PASS",
    "partial": "partial",
    "fail": "wrong output",
    "no-fix": "no fix",
}


def fix_cases_of(group: dict) -> list[tuple[int, dict, str, str]]:
    """Returns (case index, case, expected output, kind) for a group's fix cases."""
    if not group.get("fix"):
        return []
    items = []
    for i, case in enumerate(group["cases"]):
        if case["type"] != "reject":
            continue
        if case.get("unfixable"):
            items.append((i, case, case["code"], "unfixable"))
        elif isinstance(case.get("fixed"), str):
            items.append((i, case, case["fixed"], "fixed"))
    return items


def run_gale_fix_batch(
    gale_bin: Path,
    group: dict,
    items: list[tuple[int, dict, str, str]],
    timeout: int,
) -> list[dict]:
    """
    Writes each case to its own file, runs `gale --fix` over them with only the
    group's rule enabled, and reads the files back.

    Returns one dict per item: { actual, warnings, parse_error, invalid_options, error }.
    """
    rule_name = group["rule"]
    ext = SYNTAX_EXT.get(group["syntax"], ".css")
    outcomes: list[dict] = []

    with tempfile.TemporaryDirectory(prefix="gale_fix_") as tmpdir:
        tmpdir_path = Path(tmpdir)
        config_path = tmpdir_path / "gale-fix-config.json"
        config_path.write_text(json.dumps(make_config(rule_name, group["config"])))

        names = []
        for k, (_, case, _, _) in enumerate(items):
            name = f"case_{k:04d}{ext}"
            # Bytes in, bytes out: text mode would translate CRLF on read.
            (tmpdir_path / name).write_bytes(case["code"].encode("utf-8", "surrogatepass"))
            names.append(name)

        reports: dict[str, dict] = {}
        errors: dict[str, str] = {}
        batch_size = 100
        for start in range(0, len(names), batch_size):
            batch = names[start : start + batch_size]
            cmd = [str(gale_bin), "--fix", "--formatter", "json", "--config", str(config_path), *batch]
            try:
                proc = subprocess.run(cmd, cwd=tmpdir, capture_output=True, timeout=timeout)
            except subprocess.TimeoutExpired:
                errors.update({n: f"timeout after {timeout}s" for n in batch})
                continue

            stdout = proc.stdout.decode("utf-8", "replace").strip()
            try:
                parsed = json.loads(stdout) if stdout else []
            except json.JSONDecodeError:
                parsed = None
            if parsed is None or (not parsed and proc.returncode not in (0, 2)):
                stderr = proc.stderr.decode("utf-8", "replace").strip()
                detail = stderr.splitlines()[-1] if stderr else f"exit code {proc.returncode}"
                errors.update({n: detail[:200] for n in batch})
                continue
            for entry in parsed:
                reports[Path(entry.get("source", "")).name] = entry

        for k, name in enumerate(names):
            report = reports.get(name, {})
            warnings = report.get("warnings", [])
            outcomes.append({
                "actual": (tmpdir_path / name).read_bytes().decode("utf-8", "surrogatepass"),
                "warnings": [w for w in warnings if w.get("rule") == rule_name],
                "parse_error": bool(report.get("parseErrors"))
                or any(w.get("rule") in ("parse-error", "CssSyntaxError") for w in warnings),
                "invalid_options": [w.get("text", "") for w in report.get("invalidOptionWarnings", [])],
                "error": errors.get(name),
            })

    return outcomes


def judge_fix_case(case: dict, expected: str, kind: str, outcome: dict) -> tuple[bool, str]:
    actual = outcome["actual"]
    if actual == expected:
        return True, ""
    if outcome["error"]:
        return False, f"gale error: {outcome['error']}"
    if outcome["invalid_options"]:
        return False, f"gale rejected the options: {outcome['invalid_options'][0][:120]}"
    if kind == "unfixable":
        return False, "expected the code to be left unchanged (unfixable)"
    if actual == case["code"]:
        if outcome["parse_error"]:
            return False, "gale reported a parse error, so --fix skipped the file"
        if outcome["warnings"]:
            return False, "gale reported the problem but did not fix it"
        return False, "gale did not report the problem, so there was nothing to fix"
    return False, "fixed output differs from Stylelint's"


def fix_status(summary: dict) -> str:
    if summary["passed"] == summary["total"]:
        return "pass"
    if summary["fixedTotal"] > 0 and summary["changed"] == 0:
        return "no-fix"
    if summary["fixedPassed"] > 0:
        return "partial"
    return "fail"


def show(text: str, limit: int = 100) -> str:
    """One-line preview with escapes, so whitespace differences stay visible."""
    text = json.dumps(text, ensure_ascii=False)
    return text if len(text) <= limit else text[: limit - 4] + '..."'


def visible(line: str) -> str:
    line = line.replace("\t", "\\t").replace("\r", "\\r")
    stripped = line.rstrip(" ")
    return stripped + "·" * (len(line) - len(stripped))


def unified_diff(expected: str, actual: str) -> list[str]:
    diff = difflib.unified_diff(
        expected.split("\n"),
        actual.split("\n"),
        fromfile="expected (Stylelint)",
        tofile="actual (Gale)",
        lineterm="",
    )
    return [visible(line) for line in diff]


def print_fix_failure(failure: dict, verbose: bool):
    where = f"{failure['source']}/{failure['file']}"
    if failure.get("sourceLine"):
        where += f":{failure['sourceLine']}"
    print(f"    - {where}")
    print(f"      config: {json.dumps(failure['config'])}  syntax: {failure['syntax']}  [{failure['kind']}]")
    if failure.get("description"):
        print(f"      description: {failure['description']}")
    print(f"      reason:   {failure['reason']}")
    if verbose:
        print(f"      code:     {show(failure['code'], 400)}")
        for line in unified_diff(failure["expected"], failure["actual"]):
            print(f"      {line}")
    else:
        print(f"      code:     {show(failure['code'])}")
        print(f"      expected: {show(failure['expected'])}")
        print(f"      actual:   {show(failure['actual'])}")


def run_fix_mode(args, gale_bin: Path, groups: list[dict], gale_rules: set[str], requested: list[str]) -> int:
    fix_groups = [(g, fix_cases_of(g)) for g in groups]
    fix_groups = [(g, items) for g, items in fix_groups if items]

    cases_by_rule: dict[str, int] = defaultdict(int)
    source_of: dict[str, str] = {}
    for g, items in fix_groups:
        cases_by_rule[g["rule"]] += len(items)
        source_of[g["rule"]] = g["source"]

    not_implemented = sorted(r for r in cases_by_rule if r not in gale_rules)
    runnable = [(g, items) for g, items in fix_groups if g["rule"] in gale_rules]

    exit_code = 0
    known_rules = {g["rule"] for g in groups}
    for rule in requested:
        if rule not in known_rules:
            print(f"[warn] {rule} is not in {args.cases.name}")
            exit_code = 1
        elif rule in not_implemented:
            print(f"[warn] {rule} has fix cases but is not implemented in Gale")
            exit_code = 1
        elif rule not in cases_by_rule:
            print(f"[warn] {rule} has no fix cases upstream")
    if not_implemented and not requested:
        print(f"[filter] Skipping {len(not_implemented)} rules with fix cases that Gale does not implement")
        if args.verbose:
            for r in not_implemented:
                print(f"  - {r} ({cases_by_rule[r]} fix cases)")

    total_cases = sum(len(items) for _, items in runnable)
    print(f"[fix] Running {total_cases:,} fix cases across {len(runnable)} groups\n")

    rules: dict[str, dict] = {}
    t_start = time.time()
    for n, (group, items) in enumerate(runnable, 1):
        rule = group["rule"]
        summary = rules.setdefault(rule, {
            "rule": rule,
            "source": group["source"],
            "total": 0,
            "passed": 0,
            "fixedTotal": 0,
            "fixedPassed": 0,
            "unfixableTotal": 0,
            "unfixablePassed": 0,
            "changed": 0,
            "failures": [],
        })
        outcomes = run_gale_fix_batch(gale_bin, group, items, args.timeout)
        for (index, case, expected, kind), outcome in zip(items, outcomes):
            passed, reason = judge_fix_case(case, expected, kind, outcome)
            summary["total"] += 1
            summary["passed"] += passed
            summary[f"{kind}Total"] += 1
            summary[f"{kind}Passed"] += passed
            if kind == "fixed" and outcome["actual"] != case["code"]:
                summary["changed"] += 1
            if args.verbose:
                print(f"  [{'PASS' if passed else 'FAIL'}] {rule} [{kind}] {show(case['code'], 70)}")
            if not passed:
                summary["failures"].append({
                    "source": group["source"],
                    "file": group.get("file", ""),
                    "sourceLine": case.get("sourceLine"),
                    "caseIndex": index,
                    "kind": kind,
                    "config": group["config"],
                    "syntax": group["syntax"],
                    "description": case.get("description"),
                    "code": case["code"],
                    "expected": expected,
                    "actual": outcome["actual"],
                    "reason": reason,
                })
        if n % 20 == 0:
            print(f"  ... processed {n}/{len(runnable)} groups ({time.time() - t_start:.1f}s)")

    print(f"[done] Completed in {time.time() - t_start:.1f}s")

    for summary in rules.values():
        summary["status"] = fix_status(summary)

    by_status: dict[str, int] = defaultdict(int)
    for summary in rules.values():
        by_status[summary["status"]] += 1

    # Tables, one per source.
    for source in ordered_sources(s["source"] for s in rules.values()):
        rows = sorted((s for s in rules.values() if s["source"] == source), key=lambda s: s["rule"])
        if args.failing_only:
            rows = [s for s in rows if s["status"] != "pass"]
        total = sum(s["total"] for s in rules.values() if s["source"] == source)
        passed = sum(s["passed"] for s in rules.values() if s["source"] == source)
        print(f"\n{SOURCE_LABELS.get(source, source)} Autofix Compatibility")
        print("=" * 80)
        print(f"  Fix cases:           {passed:,}/{total:,} passing ({passed / total * 100 if total else 0:.1f}%)")
        if not rows:
            continue
        print(f"\n  {'Rule':<52} {'Fixed':<11} {'Unfixable':<11} Status")
        print(f"  {'-' * 84}")
        for s in rows:
            fixed = f"{s['fixedPassed']}/{s['fixedTotal']}" if s["fixedTotal"] else "-"
            unfixable = f"{s['unfixablePassed']}/{s['unfixableTotal']}" if s["unfixableTotal"] else "-"
            print(f"  {s['rule']:<52} {fixed:<11} {unfixable:<11} {FIX_STATUS_LABELS[s['status']]}")

    grand_total = sum(s["total"] for s in rules.values())
    grand_passed = sum(s["passed"] for s in rules.values())
    print("\n" + "=" * 80)
    if grand_total:
        print(f"Overall: {grand_passed:,}/{grand_total:,} ({grand_passed / grand_total * 100:.1f}%) fix cases passing")
    print(
        f"Rules: {by_status['pass']} pass, {by_status['partial']} partial, "
        f"{by_status['fail']} wrong output, {by_status['no-fix']} no fix"
        + (f", {len(not_implemented)} not implemented" if not requested else "")
    )
    print("=" * 80)

    # Failure details: all of them for the rules asked for, a sample otherwise.
    if requested or args.failing_only or args.details or args.verbose:
        limit = None if requested else 5
        failing = [s for s in sorted(rules.values(), key=lambda s: s["rule"]) if s["failures"]]
        if failing:
            print("\nFix Failures")
            print("=" * 80)
        for s in failing:
            print(f"\n  {s['rule']} ({s['passed']}/{s['total']} passing):")
            for failure in s["failures"][:limit]:
                print_fix_failure(failure, args.verbose)
            hidden = len(s["failures"]) - len(s["failures"][:limit])
            if hidden:
                print(f"    ... and {hidden} more (run with --rule {s['rule']} to see all)")

    if args.json:
        report = {
            "mode": "fix",
            "generatedAt": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "binary": str(gale_bin),
            "testCases": str(args.cases),
            "filters": {"rules": requested, "source": args.source},
            "summary": {
                "rules": len(rules),
                "cases": grand_total,
                "passed": grand_passed,
                "failed": grand_total - grand_passed,
                "byStatus": {k: by_status.get(k, 0) for k in FIX_STATUS_LABELS},
            },
            "rules": sorted(rules.values(), key=lambda s: (source_rank(s["source"]), s["rule"])),
            "notImplemented": [
                {"rule": r, "source": source_of[r], "cases": cases_by_rule[r]} for r in not_implemented
            ],
        }
        write_json(args.json, report)

    if grand_total != grand_passed:
        exit_code = 1
    return exit_code


def write_json(path: str, data: dict):
    out = Path(path)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
    print(f"\n[save] JSON report written to {out}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main():
    parser = argparse.ArgumentParser(
        description="Stylelint compatibility test runner for Gale"
    )
    parser.add_argument(
        "--rule",
        action="append",
        help="Test only this rule (repeatable, or comma-separated)",
    )
    parser.add_argument(
        "--source",
        type=str,
        help="Test only this source (stylelint, stylelint-scss, stylelint-order)",
    )
    parser.add_argument(
        "--fix",
        action="store_true",
        help="Check autofix output against Stylelint's `fixed` instead of warnings",
    )
    parser.add_argument(
        "--failing-only", action="store_true", help="Show only failing rules"
    )
    parser.add_argument(
        "--skip-build", action="store_true", help="Skip building Gale"
    )
    parser.add_argument(
        "--binary",
        "--gale-bin",
        dest="gale_bin",
        type=str,
        help="Path to a pre-built Gale binary (implies --skip-build)",
    )
    parser.add_argument(
        "--cases",
        type=Path,
        default=TEST_CASES_FILE,
        help="Path to test-cases.json (default: next to this script)",
    )
    parser.add_argument(
        "--json", type=str, metavar="PATH", help="Also write a JSON report here"
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=60,
        help="Seconds to allow each Gale invocation in --fix mode (default 60)",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Show every case result (in --fix mode: a diff per failure)",
    )
    parser.add_argument(
        "--details", action="store_true", help="Show detailed failure info"
    )
    args = parser.parse_args()
    requested = [
        r.strip() for arg in (args.rule or []) for r in arg.split(",") if r.strip()
    ]

    # Load test cases
    if not args.cases.exists():
        print(f"[error] {args.cases} not found.")
        print("Run 'bun extract.mjs' first to extract test cases.")
        sys.exit(1)

    with open(args.cases) as f:
        test_groups = json.load(f)

    print(f"[load] Loaded {len(test_groups)} test groups from {args.cases.name}")

    sources = {g["source"] for g in test_groups}
    if args.source and args.source not in sources:
        print(f"[error] Unknown source {args.source!r}; choose from {', '.join(ordered_sources(sources))}")
        sys.exit(1)

    # Build or locate Gale
    if args.gale_bin:
        # Absolute, because Gale runs from a temp directory.
        gale_bin = Path(args.gale_bin).expanduser().resolve()
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
    if requested:
        filtered = [g for g in filtered if g["rule"] in requested]

    if args.fix:
        sys.exit(run_fix_mode(args, gale_bin, filtered, gale_rules, requested))

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

    for source in ordered_sources(rule_results_by_source):
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

    if args.json:
        write_json(args.json, {
            "mode": "lint",
            "generatedAt": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "binary": str(gale_bin),
            "testCases": str(args.cases),
            "filters": {"rules": requested, "source": args.source},
            "summary": {
                "cases": grand_total,
                "passed": grand_passing,
                "failed": grand_total - grand_passing,
            },
            "rules": [
                {"rule": rule, "source": source, **counts}
                for source in ordered_sources(results_data)
                for rule, counts in sorted(results_data[source].items())
            ],
        })


if __name__ == "__main__":
    main()
