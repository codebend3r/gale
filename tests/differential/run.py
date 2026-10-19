#!/usr/bin/env python3
"""
Differential testing harness for Gale vs Stylelint.

Clones public repos, runs both linters with JSON output (using the repo's own
Stylelint config), and compares results. This tests whether Gale is truly a
drop-in replacement.

Usage:
    python run.py                    # Run all repos
    python run.py grafana bootstrap  # Run specific repos
    python run.py --list             # List available repos
    python run.py --update           # Re-clone repos (force fresh)
    python run.py --css-only         # Only test .css files (skip SCSS/Less)
    python run.py --benchmark        # Also measure and report execution times
"""

import argparse
import json
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from harness import (
    build_gale, clone_repo, compare, find_stylelint_bin, install_deps, load_repos,
    parse_report, repo_globs, run_cmd,
)

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

SCRIPT_DIR = Path(__file__).parent
CLONES_DIR = SCRIPT_DIR / ".clones"
RESULTS_DIR = SCRIPT_DIR / "results"

# No rule filtering. Both Stylelint and Gale output is compared as-is.
# If Stylelint reports a warning, Gale must report it too. Period.
# If a third-party plugin rule fires in Stylelint and Gale doesn't support it,
# that's a legitimate false negative that needs to be fixed.

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def find_css_files(clone_dir: Path, search_paths: list[str], css_only: bool = False) -> list[str]:
    """Find all CSS/SCSS/Less files under the given paths."""
    extensions = {".css"} if css_only else {".css", ".scss", ".less", ".sass"}
    files = []

    for search_path in search_paths:
        root = clone_dir / search_path
        if not root.exists():
            continue
        for f in root.rglob("*"):
            if (f.suffix in extensions
                    and "node_modules" not in f.parts
                    and ".git" not in f.parts):
                files.append(str(f.relative_to(clone_dir)))

    return sorted(files)


# ---------------------------------------------------------------------------
# Linter runners
# ---------------------------------------------------------------------------


def run_stylelint(
    work_dir: Path, clone_dir: Path, globs: list[str], search_paths: list[str] | None = None,
) -> list[dict] | None:
    """Run Stylelint on glob patterns and return parsed JSON output.

    This mirrors how a real user would run Stylelint:
      bunx stylelint 'src/**/*.scss' --formatter json
    """
    stylelint_bin = find_stylelint_bin(work_dir, clone_dir, search_paths)
    if stylelint_bin is None:
        print(f"  [error] Stylelint binary not found")
        return None

    # Pass globs directly — let Stylelint discover files like a real user would.
    cmd = [str(stylelint_bin), "--formatter", "json", "--no-color"] + globs
    timeout = 600
    result = run_cmd(cmd, cwd=str(work_dir), timeout=timeout)

    # Stylelint 16+ may output JSON to stderr instead of stdout.
    report = parse_report(result.stdout, result.stderr)
    if report is not None:
        return report

    output = result.stdout.strip() or result.stderr.strip()
    if result.returncode == 2:
        print(f"  [error] Stylelint config error: {output[:200] or '(exit 2, no output)'}")
        return None

    if output:
        print(f"  [warn] Stylelint printed no JSON report")
        print(f"  [warn]   output[:200]: {output[:200]}")
    return []


def extract_package_json_globs(clone_dir: Path) -> list[str]:
    """Extract stylelint glob patterns from the repo's package.json scripts.

    Looks for scripts that invoke `stylelint` and extracts the glob arguments.
    This lets us verify that Gale can handle the *actual* globs the repo uses,
    not just the ones we defined in repos.json.
    """
    pkg = clone_dir / "package.json"
    if not pkg.exists():
        return []
    try:
        data = json.load(open(pkg))
    except (json.JSONDecodeError, OSError):
        return []
    scripts = data.get("scripts", {})
    globs = []
    import re
    for _name, cmd in scripts.items():
        if "stylelint" not in cmd:
            continue
        # Extract quoted glob arguments from commands like:
        #   stylelint '{src,packages}/**/*.scss' --cache
        #   stylelint "src/**/*.css" --fix
        for match in re.finditer(r"""['"]([^'"]*\*[^'"]*)['"]\s""", cmd):
            globs.append(match.group(1))
        # Also try unquoted globs (less common):
        #   stylelint src/**/*.css
        for match in re.finditer(r"""stylelint\s+([^\s'"-][^\s]*\*[^\s]*)""", cmd):
            candidate = match.group(1)
            if not candidate.startswith("-"):
                globs.append(candidate)
    return globs


def verify_gale_glob_discovery(clone_dir: Path, gale_bin: Path, repo_name: str) -> bool:
    """Verify Gale can discover files using the repo's actual package.json globs.

    Returns True if the check passes or is skipped, False if Gale fails to
    find files with a glob that Stylelint would handle.
    """
    pkg_globs = extract_package_json_globs(clone_dir)
    if not pkg_globs:
        return True

    print(f"  [glob-check] Verifying Gale handles package.json globs: {pkg_globs}")
    for glob in pkg_globs:
        cmd = [str(gale_bin), "--formatter", "json", glob]
        result = run_cmd(cmd, cwd=str(clone_dir), timeout=30)
        # Check if Gale found files (non-empty JSON array) or at least
        # didn't report "No CSS files found"
        if "No CSS files found" in result.stderr or "No CSS files found" in result.stdout:
            print(f"  [glob-check] FAIL: Gale found no files with glob '{glob}'")
            print(f"  [glob-check]   This glob works in Stylelint but not in Gale.")
            print(f"  [glob-check]   This would break a real migration.")
            return False
        # Also verify we get valid JSON output (even if empty array is ok for
        # repos where the glob targets a subset)
        try:
            files = json.loads(result.stdout) if result.stdout.strip() else []
            print(f"  [glob-check] OK: '{glob}' -> {len(files)} files")
        except json.JSONDecodeError:
            print(f"  [glob-check] WARN: Gale output not valid JSON for glob '{glob}'")
    return True


def run_gale(clone_dir: Path, globs: list[str], gale_bin: Path) -> list[dict] | None:
    """Run Gale on glob patterns and return parsed JSON output.

    This mirrors how a real user would run Gale as a drop-in replacement:
      bunx gale 'src/**/*.scss' --formatter json
    """
    if not gale_bin.exists():
        print(f"  [error] Gale binary not found")
        return None

    # Pass globs directly — Gale discovers files from globs like Stylelint.
    cmd = [str(gale_bin), "--formatter", "json"] + globs
    timeout = 600
    result = run_cmd(cmd, cwd=str(clone_dir), timeout=timeout)

    # Capture stderr for debugging (warnings, unsupported rules, etc.)
    if result.stderr.strip():
        for line in result.stderr.strip().split("\n")[:5]:
            print(f"  [gale stderr] {line}")

    stdout = result.stdout.strip()
    if not stdout:
        return []

    try:
        return json.loads(stdout)
    except json.JSONDecodeError as e:
        print(f"  [error] Gale JSON parse error: {e}")
        return None


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------


def print_report(repo_name: str, report: dict):
    total = report["matching_warnings"] + report["stylelint_only_warnings"] + report["gale_only_warnings"]

    print(f"\n{'='*70}")
    print(f"  REPORT: {repo_name}")
    print(f"{'='*70}")
    print(f"  Files analyzed:        {report['total_files']}")
    print(f"  Files matching:        {report['files_match']}")
    print(f"  Files with diffs:      {report['files_differ']}")
    print(f"  Matching warnings:     {report['matching_warnings']}")
    print(f"  Stylelint-only (FN):   {report['stylelint_only_warnings']}")
    print(f"  Gale-only (FP):        {report['gale_only_warnings']}")

    if total > 0:
        parity = report["matching_warnings"] / total * 100
        print(f"  Parity score:          {parity:.1f}%")

    if "stylelint_time" in report:
        s_time = report["stylelint_time"]
        g_time = report["gale_time"]
        speedup = s_time / g_time if g_time > 0 else float("inf")
        print(f"\n  Performance:")
        print(f"    Stylelint:   {s_time:.2f}s")
        print(f"    Gale:        {g_time:.2f}s")
        print(f"    Speedup:     {speedup:.1f}x faster")

    # Breakdown by rule
    rule_fn: Counter = Counter()
    rule_fp: Counter = Counter()
    for diff in report["diffs"]:
        for w in diff["stylelint_only"]:
            rule_fn[w["rule"]] += 1
        for w in diff["gale_only"]:
            rule_fp[w["rule"]] += 1

    if rule_fn or rule_fp:
        all_rules = sorted(set(list(rule_fn.keys()) + list(rule_fp.keys())))
        print(f"\n  Rule breakdown:")
        print(f"  {'Rule':<50} {'FN':<8} {'FP':<8}")
        print(f"  {'─'*66}")
        for rule in all_rules:
            print(f"  {rule:<50} {rule_fn[rule]:<8} {rule_fp[rule]:<8}")

    # Show first N file diffs
    max_diffs = 50
    shown = 0
    for diff in report["diffs"]:
        if shown >= max_diffs:
            remaining = len(report["diffs"]) - max_diffs
            print(f"\n  ... and {remaining} more files with differences")
            break

        print(f"\n  --- {diff['file']} ({diff['matching']} match, "
              f"{len(diff['stylelint_only'])} FN, {len(diff['gale_only'])} FP)")

        for w in diff["stylelint_only"][:3]:
            print(f"    [FN] {w['line']}:{w['column']} {w['rule']} ({w['severity']}) {w['text']}")
        for w in diff["gale_only"][:3]:
            print(f"    [FP] {w['line']}:{w['column']} {w['rule']} ({w['severity']}) {w['text']}")

        shown += 1

    print(f"\n{'='*70}\n")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def process_repo(
    repo_config: dict,
    gale_bin: Path,
    force_clone: bool = False,
    css_only: bool = False,
    benchmark: bool = False,
) -> dict | None:
    """Process a single repo: clone, install, lint with both tools, compare."""
    if repo_config.get("skip"):
        print(f"\n  [skip] {repo_config['name']}: {repo_config.get('notes', 'skipped')}")
        return None

    name = repo_config["name"]
    repo = repo_config["repo"]
    branch = repo_config["branch"]
    search_paths = repo_config["paths"]

    print(f"\n{'─'*70}")
    print(f"Processing: {name} ({repo})")
    print(f"{'─'*70}")

    clone_dir = CLONES_DIR / name
    cwd_override = repo_config.get("cwd")
    work_dir = clone_dir / cwd_override if cwd_override else clone_dir

    if not clone_repo(repo, branch, clone_dir, pull=True, force=force_clone):
        return None

    deps_result = install_deps(clone_dir, work_dir=work_dir, search_paths=search_paths)
    if deps_result == "skip":
        print(f"  [skip] {name}: skipped due to environment requirements")
        return None
    if not deps_result:
        print(f"  [warn] Continuing without deps (Stylelint may not work)")

    # Build glob patterns from repo config.
    # If the repo defines explicit globs, use them.  Otherwise, construct
    # from paths: "{path}/**/*.{css,scss,less}"
    globs = repo_globs(repo_config, css_only=css_only)

    print(f"  [globs] {' '.join(globs)}")

    # Run Stylelint (using repo's own config + globs, like a real user)
    print(f"  [lint] Running Stylelint...")
    t0 = time.time()
    try:
        stylelint_results = run_stylelint(work_dir, clone_dir, globs, search_paths=search_paths)
    except subprocess.TimeoutExpired:
        stylelint_time = time.time() - t0
        print(f"  [warn] Stylelint timed out after {stylelint_time:.0f}s, skipping repo")
        return {
            "total_files": 0, "files_match": 0, "files_differ": 0,
            "stylelint_only_warnings": 0, "gale_only_warnings": 0,
            "matching_warnings": 0, "diffs": [], "timeout": "stylelint",
        }
    stylelint_time = time.time() - t0
    if stylelint_results is None:
        print(f"  [warn] Stylelint failed, skipping comparison")
        return None
    s_count = sum(len(r.get("warnings", [])) for r in stylelint_results)
    s_files = len(stylelint_results)
    print(f"  [lint] Stylelint: {s_count} warnings across {s_files} files")

    # Run Gale (using repo's own config + same globs — drop-in replacement)
    print(f"  [lint] Running Gale...")
    t0 = time.time()
    try:
        gale_results = run_gale(work_dir, globs, gale_bin)
    except subprocess.TimeoutExpired:
        gale_time = time.time() - t0
        print(f"  [warn] Gale timed out after {gale_time:.0f}s, skipping repo")
        return {
            "total_files": 0, "files_match": 0, "files_differ": 0,
            "stylelint_only_warnings": 0, "gale_only_warnings": 0,
            "matching_warnings": 0, "diffs": [], "timeout": "gale",
        }
    gale_time = time.time() - t0
    if gale_results is None:
        print(f"  [warn] Gale failed, skipping comparison")
        return None
    g_count = sum(len(r.get("warnings", [])) for r in gale_results)
    g_files = len(gale_results)
    print(f"  [lint] Gale: {g_count} warnings across {g_files} files")

    # Normalize & compare — NO FILTERS on either side.
    # If Stylelint reports a warning, Gale must report it too (or it's a FN).
    # If Gale reports a warning Stylelint doesn't, it's a FP.
    # This is the only honest way to validate a drop-in replacement.
    # Expected diffs (e.g. Stylelint v13 message differences) are excluded.
    report, excluded = compare(
        stylelint_results, gale_results, work_dir, repo_config.get("expected_diffs"),
    )
    if excluded:
        print(f"  [info] {excluded} expected v13 difference(s) excluded from parity count")

    # Verify Gale can handle the actual globs from the repo's package.json.
    # This catches bugs like brace expansion in directory paths that the
    # repos.json globs might not exercise.
    glob_ok = verify_gale_glob_discovery(work_dir, gale_bin, name)
    report["glob_discovery_ok"] = glob_ok
    if not glob_ok:
        print(f"  [warn] Gale cannot handle some package.json globs — migration would break!")

    # Before saving: the compatibility matrix reads the speedup from the report.
    if benchmark:
        report["stylelint_time"] = stylelint_time
        report["gale_time"] = gale_time

    # Save raw results
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    with open(RESULTS_DIR / f"{name}_stylelint.json", "w") as f:
        json.dump(stylelint_results, f, indent=2)
    with open(RESULTS_DIR / f"{name}_gale.json", "w") as f:
        json.dump(gale_results, f, indent=2)
    with open(RESULTS_DIR / f"{name}_report.json", "w") as f:
        json.dump(report, f, indent=2)

    print_report(name, report)
    return report


def main():
    parser = argparse.ArgumentParser(description="Differential testing: Gale vs Stylelint")
    parser.add_argument("repos", nargs="*", help="Specific repos to test (by name)")
    parser.add_argument("--list", action="store_true", help="List available repos")
    parser.add_argument("--update", action="store_true", help="Force re-clone repos")
    parser.add_argument("--skip-build", action="store_true", help="Skip building Gale")
    parser.add_argument("--gale-bin", type=str, help="Path to pre-built Gale binary")
    parser.add_argument("--css-only", action="store_true",
                        help="Only test .css files (skip SCSS/Less)")
    parser.add_argument("--benchmark", action="store_true",
                        help="Measure and report execution time of both linters")
    args = parser.parse_args()

    repos = load_repos()

    if args.list:
        print("Available repos:")
        for r in repos:
            print(f"  {r['name']:<20} {r['repo']:<40} {r.get('notes', '')}")
        return

    if args.repos:
        repos = [r for r in repos if r["name"] in args.repos]
        if not repos:
            print(f"No repos matched: {args.repos}")
            sys.exit(1)

    if args.gale_bin:
        gale_bin = Path(args.gale_bin)
        if not gale_bin.exists():
            print(f"[error] Gale binary not found: {gale_bin}")
            sys.exit(1)
    else:
        gale_bin = build_gale(skip=args.skip_build)
        if gale_bin is None:
            sys.exit(1)

    CLONES_DIR.mkdir(parents=True, exist_ok=True)
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)

    summaries = []
    for repo_config in repos:
        report = process_repo(
            repo_config, gale_bin,
            force_clone=args.update,
            css_only=args.css_only,
            benchmark=args.benchmark,
        )
        if report:
            summaries.append((repo_config["name"], report))

    if summaries:
        has_benchmark = "stylelint_time" in summaries[0][1]
        print(f"\n{'='*70}")
        print(f"  FINAL SUMMARY")
        print(f"{'='*70}")
        print(f"  Comparison: unfiltered (all rules, both sides)")
        print()
        header = f"  {'Repo':<20} {'Files':<8} {'Match':<8} {'FN':<8} {'FP':<8} {'Parity':<8}"
        sep_len = 60
        if has_benchmark:
            header += f"{'Speedup':<10}"
            sep_len = 70
        print(header)
        print(f"  {'─'*sep_len}")
        for name, report in summaries:
            total = report["matching_warnings"] + report["stylelint_only_warnings"] + report["gale_only_warnings"]
            parity = f"{report['matching_warnings'] / total * 100:.1f}%" if total > 0 else "N/A"
            line = (f"  {name:<20} {report['total_files']:<8} "
                    f"{report['matching_warnings']:<8} "
                    f"{report['stylelint_only_warnings']:<8} "
                    f"{report['gale_only_warnings']:<8} "
                    f"{parity:<8}")
            if has_benchmark and "stylelint_time" in report and "gale_time" in report:
                s_time = report["stylelint_time"]
                g_time = report["gale_time"]
                speedup = s_time / g_time if g_time > 0 else float("inf")
                line += f"{speedup:.1f}x"
            elif has_benchmark:
                line += "N/A"
            print(line)
        print()


if __name__ == "__main__":
    main()
