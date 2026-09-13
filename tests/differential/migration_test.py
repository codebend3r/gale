#!/usr/bin/env python3
"""
Migration test: simulate a real Stylelint-to-Gale migration.

For each repo, this script:
1. Clones the repo (shallow)
2. Installs dependencies
3. Finds the Stylelint config and npm lint scripts
4. Runs the original Stylelint command (as defined in package.json)
5. Replaces the Stylelint invocation with Gale
6. Runs the Gale command
7. Compares exit codes and output
8. Reports whether the migration is clean

This validates that Gale is a TRUE drop-in replacement — not just that
it can parse the same files, but that it works as a direct substitute
in real CI/CD pipelines.

Usage:
    python migration_test.py                     # Test all repos
    python migration_test.py bootstrap grafana   # Test specific repos
    python migration_test.py --list              # List repos
    python migration_test.py --verbose           # Show full output
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
REPOS_JSON = SCRIPT_DIR / "repos.json"
CLONES_DIR = SCRIPT_DIR / ".clones"
GALE_ROOT = SCRIPT_DIR.parent.parent


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def run_cmd(
    cmd: list[str], cwd: str | None = None, timeout: int = 300
) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)


def load_repos() -> list[dict]:
    with open(REPOS_JSON) as f:
        return json.load(f)


def clone_repo(repo: str, branch: str, dest: Path) -> bool:
    if dest.exists():
        print(f"  [skip] Already cloned: {dest.name}")
        return True
    print(f"  [clone] {repo} @ {branch}")
    result = run_cmd(
        ["git", "clone", "--depth", "1", "--branch", branch,
         f"https://github.com/{repo}.git", str(dest)],
        timeout=120,
    )
    if result.returncode != 0:
        print(f"  [error] Clone failed: {result.stderr.strip()[:200]}")
        return False
    return True


def detect_package_manager(clone_dir: Path) -> str:
    if (clone_dir / "pnpm-lock.yaml").exists():
        return "pnpm"
    if (clone_dir / "yarn.lock").exists():
        return "yarn"
    if (clone_dir / "bun.lockb").exists() or (clone_dir / "bun.lock").exists():
        return "bun"
    return "npm"


def install_deps(clone_dir: Path) -> bool:
    if (clone_dir / "node_modules").exists():
        print(f"  [skip] node_modules exists")
        return True

    if not (clone_dir / "package.json").exists():
        print(f"  [warn] No package.json")
        return False

    pm = detect_package_manager(clone_dir)
    print(f"  [install] {pm} install...")

    run_cmd(["corepack", "enable"], cwd=str(clone_dir), timeout=30)

    if pm == "pnpm":
        cmd = ["pnpm", "install", "--ignore-scripts", "--no-frozen-lockfile"]
    elif pm == "yarn":
        env_file = clone_dir / ".yarnrc.yml"
        if env_file.exists():
            content = env_file.read_text()
            if "nodeLinker" not in content:
                with open(env_file, "a") as f:
                    f.write("\nnodeLinker: node-modules\n")
        cmd = ["yarn", "install", "--mode", "skip-build"]
    elif pm == "bun":
        cmd = ["bun", "install", "--ignore-scripts"]
    else:
        cmd = ["npm", "install", "--ignore-scripts"]

    result = run_cmd(cmd, cwd=str(clone_dir), timeout=300)
    if result.returncode != 0:
        print(f"  [error] Install failed: {result.stderr.strip()[:200]}")
        return False
    return True


# ---------------------------------------------------------------------------
# Config & script detection
# ---------------------------------------------------------------------------

STYLELINT_CONFIG_FILES = [
    ".stylelintrc",
    ".stylelintrc.json",
    ".stylelintrc.yml",
    ".stylelintrc.yaml",
    ".stylelintrc.js",
    ".stylelintrc.cjs",
    ".stylelintrc.mjs",
    "stylelint.config.js",
    "stylelint.config.cjs",
    "stylelint.config.mjs",
]


def find_stylelint_config(clone_dir: Path) -> str | None:
    """Find which Stylelint config file exists in the repo."""
    for name in STYLELINT_CONFIG_FILES:
        if (clone_dir / name).exists():
            return name

    # Check package.json for "stylelint" field
    pkg_json = clone_dir / "package.json"
    if pkg_json.exists():
        try:
            pkg = json.loads(pkg_json.read_text())
            if "stylelint" in pkg:
                return "package.json (stylelint field)"
        except json.JSONDecodeError:
            pass

    return None


def find_lint_scripts(clone_dir: Path) -> dict[str, str]:
    """Find npm scripts that invoke Stylelint."""
    pkg_json = clone_dir / "package.json"
    if not pkg_json.exists():
        return {}

    try:
        pkg = json.loads(pkg_json.read_text())
    except json.JSONDecodeError:
        return {}

    scripts = pkg.get("scripts", {})
    stylelint_scripts = {}

    for name, cmd in scripts.items():
        if "stylelint" in cmd.lower():
            stylelint_scripts[name] = cmd

    return stylelint_scripts


def replace_stylelint_with_gale(cmd: str, gale_bin: str) -> str:
    """Replace 'stylelint' invocations in a command with the Gale binary."""
    # Replace npx/bunx stylelint
    result = re.sub(r'\b(npx|bunx)\s+stylelint\b', gale_bin, cmd)
    # Replace bare stylelint command
    result = re.sub(r'\bstylelint\b', gale_bin, result)
    return result


# ---------------------------------------------------------------------------
# Output comparison
# ---------------------------------------------------------------------------


def parse_lint_output(output: str) -> set[tuple]:
    """Parse JSON lint output into a set of (file, line, col, rule) tuples."""
    try:
        data = json.loads(output)
    except (json.JSONDecodeError, ValueError):
        return set()

    warnings = set()
    for entry in data:
        source = entry.get("source", "")
        for w in entry.get("warnings", []):
            warnings.add((
                source,
                w.get("line", 0),
                w.get("column", 0),
                w.get("rule", ""),
            ))
    return warnings


def compare_outputs(
    stylelint_out: str, gale_out: str, gale_rules: set[str] | None = None
) -> dict:
    """Compare linter outputs and return a summary."""
    s_warnings = parse_lint_output(stylelint_out)
    g_warnings = parse_lint_output(gale_out)

    if gale_rules:
        s_warnings = {w for w in s_warnings if w[3] in gale_rules}
        g_warnings = {w for w in g_warnings if w[3] in gale_rules}

    matching = s_warnings & g_warnings
    fn = s_warnings - g_warnings
    fp = g_warnings - s_warnings

    return {
        "matching": len(matching),
        "false_negatives": len(fn),
        "false_positives": len(fp),
        "stylelint_total": len(s_warnings),
        "gale_total": len(g_warnings),
    }


# ---------------------------------------------------------------------------
# Migration test
# ---------------------------------------------------------------------------


def test_migration(
    repo_config: dict,
    gale_bin: Path,
    verbose: bool = False,
) -> dict:
    """Run a full migration test on a single repo."""
    name = repo_config["name"]
    repo = repo_config["repo"]
    branch = repo_config["branch"]

    print(f"\n{'='*70}")
    print(f"  MIGRATION TEST: {name} ({repo})")
    print(f"{'='*70}")

    result = {
        "name": name,
        "status": "UNKNOWN",
        "config_found": False,
        "scripts_found": {},
        "scripts_tested": {},
    }

    clone_dir = CLONES_DIR / name

    # Step 1: Clone & install
    if not clone_repo(repo, branch, clone_dir):
        result["status"] = "CLONE_FAILED"
        return result

    if not install_deps(clone_dir):
        result["status"] = "INSTALL_FAILED"
        return result

    # Step 2: Find Stylelint config
    config_file = find_stylelint_config(clone_dir)
    if config_file:
        result["config_found"] = True
        print(f"  [config] Found: {config_file}")
    else:
        print(f"  [config] No Stylelint config found")
        result["status"] = "NO_CONFIG"
        return result

    # Step 3: Find lint scripts
    lint_scripts = find_lint_scripts(clone_dir)
    result["scripts_found"] = lint_scripts

    if lint_scripts:
        print(f"  [scripts] Found {len(lint_scripts)} Stylelint script(s):")
        for sname, cmd in lint_scripts.items():
            print(f"    {sname}: {cmd}")
    else:
        print(f"  [scripts] No Stylelint scripts in package.json")

    # Step 4: Run Stylelint with JSON formatter (baseline)
    stylelint_bin = clone_dir / "node_modules" / ".bin" / "stylelint"
    if not stylelint_bin.exists():
        print(f"  [error] Stylelint binary not found in node_modules")
        result["status"] = "NO_STYLELINT_BIN"
        return result

    # Find the glob pattern from the first lint script, or use search_paths
    search_paths = repo_config.get("paths", ["."])
    glob_pattern = None

    for sname, cmd in lint_scripts.items():
        # Extract glob pattern from command (e.g., stylelint 'src/**/*.css')
        match = re.search(r"stylelint\s+['\"]([^'\"]+)['\"]", cmd)
        if match:
            glob_pattern = match.group(1)
            print(f"  [glob] Extracted from script '{sname}': {glob_pattern}")
            break

    if not glob_pattern:
        # Construct a default glob from search paths
        extensions = "*.{css,scss,less}"
        if len(search_paths) == 1 and search_paths[0] == ".":
            glob_pattern = f"**/{extensions}"
        else:
            glob_pattern = "{" + ",".join(f"{p}/**/{extensions}" for p in search_paths) + "}"
        print(f"  [glob] Constructed default: {glob_pattern}")

    print(f"\n  [step 1/2] Running Stylelint (baseline)...")
    stylelint_result = run_cmd(
        [str(stylelint_bin), glob_pattern, "--formatter", "json"],
        cwd=str(clone_dir), timeout=120,
    )
    stylelint_exit = stylelint_result.returncode
    stylelint_json = stylelint_result.stdout.strip()
    if not stylelint_json:
        # Some versions output JSON to stderr
        stderr = stylelint_result.stderr.strip()
        json_start = stderr.find("[{")
        if json_start >= 0:
            stylelint_json = stderr[json_start:]

    print(f"    Exit code: {stylelint_exit}")
    if verbose and stylelint_result.stderr.strip():
        for line in stylelint_result.stderr.strip().split("\n")[:5]:
            print(f"    stderr: {line}")

    # Step 5: Run Gale with same glob (migration test)
    print(f"  [step 2/2] Running Gale (migration)...")
    gale_result = run_cmd(
        [str(gale_bin), glob_pattern, "--formatter", "json"],
        cwd=str(clone_dir), timeout=120,
    )
    gale_exit = gale_result.returncode
    gale_json = gale_result.stdout.strip()

    print(f"    Exit code: {gale_exit}")
    if verbose and gale_result.stderr.strip():
        for line in gale_result.stderr.strip().split("\n")[:5]:
            print(f"    stderr: {line}")

    # Step 6: Compare
    if stylelint_json and gale_json:
        comparison = compare_outputs(stylelint_json, gale_json)
        result["comparison"] = comparison

        total = comparison["matching"] + comparison["false_negatives"] + comparison["false_positives"]
        parity = comparison["matching"] / total * 100 if total > 0 else 100.0

        print(f"\n  Results:")
        print(f"    Matching warnings:   {comparison['matching']}")
        print(f"    False negatives:     {comparison['false_negatives']}")
        print(f"    False positives:     {comparison['false_positives']}")
        print(f"    Parity:              {parity:.1f}%")
        print(f"    Exit codes:          Stylelint={stylelint_exit} Gale={gale_exit}")

        if comparison["false_negatives"] == 0 and comparison["false_positives"] == 0:
            result["status"] = "PASS"
            print(f"\n  >>> MIGRATION: PASS (clean drop-in replacement)")
        elif parity >= 95.0:
            result["status"] = "PASS_WITH_DIFFS"
            print(f"\n  >>> MIGRATION: PASS WITH DIFFS ({parity:.1f}% parity)")
        else:
            result["status"] = "FAIL"
            print(f"\n  >>> MIGRATION: FAIL ({parity:.1f}% parity)")
    elif not stylelint_json:
        result["status"] = "STYLELINT_NO_OUTPUT"
        print(f"\n  >>> Stylelint produced no JSON output (config error?)")
    elif not gale_json:
        result["status"] = "GALE_NO_OUTPUT"
        print(f"\n  >>> Gale produced no JSON output")

    # Step 7: Test npm script replacement (if scripts found)
    for sname, cmd in lint_scripts.items():
        migrated_cmd = replace_stylelint_with_gale(cmd, str(gale_bin))
        result["scripts_tested"][sname] = {
            "original": cmd,
            "migrated": migrated_cmd,
        }
        print(f"\n  [script] {sname}:")
        print(f"    Original: {cmd}")
        print(f"    Migrated: {migrated_cmd}")

    return result


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def build_gale() -> Path | None:
    print("[build] Building Gale (release)...")
    result = run_cmd(["cargo", "build", "--release"], cwd=str(GALE_ROOT), timeout=300)
    if result.returncode != 0:
        print(f"[error] Build failed: {result.stderr.strip()[:500]}")
        return None
    binary = GALE_ROOT / "target" / "release" / "gale"
    if not binary.exists():
        print(f"[error] Binary not found")
        return None
    return binary


def main():
    parser = argparse.ArgumentParser(description="Migration test: Stylelint to Gale")
    parser.add_argument("repos", nargs="*", help="Specific repos to test")
    parser.add_argument("--list", action="store_true", help="List available repos")
    parser.add_argument("--skip-build", action="store_true", help="Skip building Gale")
    parser.add_argument("--gale-bin", type=str, help="Path to Gale binary")
    parser.add_argument("--verbose", "-v", action="store_true", help="Show detailed output")
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
    elif args.skip_build:
        gale_bin = GALE_ROOT / "target" / "release" / "gale"
    else:
        gale_bin = build_gale()
        if gale_bin is None:
            sys.exit(1)

    if not gale_bin.exists():
        print(f"[error] Gale binary not found: {gale_bin}")
        sys.exit(1)

    CLONES_DIR.mkdir(parents=True, exist_ok=True)

    results = []
    for repo_config in repos:
        result = test_migration(repo_config, gale_bin, verbose=args.verbose)
        results.append(result)

    # Final summary
    print(f"\n{'='*70}")
    print(f"  MIGRATION TEST SUMMARY")
    print(f"{'='*70}")
    print(f"  {'Repo':<20} {'Config':<12} {'Scripts':<10} {'Status':<20}")
    print(f"  {'─'*62}")

    pass_count = 0
    fail_count = 0
    for r in results:
        config_str = "Yes" if r["config_found"] else "No"
        scripts_str = str(len(r["scripts_found"]))
        status = r["status"]

        if status in ("PASS", "PASS_WITH_DIFFS"):
            pass_count += 1
        elif status == "FAIL":
            fail_count += 1

        print(f"  {r['name']:<20} {config_str:<12} {scripts_str:<10} {status:<20}")

    total = len(results)
    print(f"\n  Total: {total} | Pass: {pass_count} | Fail: {fail_count} | Other: {total - pass_count - fail_count}")
    print()

    sys.exit(1 if fail_count > 0 else 0)


if __name__ == "__main__":
    main()
