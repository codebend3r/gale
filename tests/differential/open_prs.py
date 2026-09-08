#!/usr/bin/env python3
"""
Open PRs to migrate repos from Stylelint to Gale.

For each repo:
1. Run differential test (must be 0 FP / 0 FN)
2. Measure benchmark times
3. Fork the repo
4. Modify package.json (swap stylelint → gale in devDeps and scripts)
5. Commit, push, open PR

Usage:
    python open_prs.py bootstrap grafana       # Specific repos
    python open_prs.py --all                   # All repos that pass
    python open_prs.py --dry-run bootstrap     # Show what would change without opening PR
    python open_prs.py --list                  # List available repos
"""

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
REPOS_JSON = SCRIPT_DIR / "repos.json"
CLONES_DIR = SCRIPT_DIR / ".clones"
GALE_ROOT = SCRIPT_DIR.parent.parent
GALE_VERSION = "0.1.4"
GALE_NPM_PACKAGE = "@codebend3r/gale"
GALE_GITHUB = "https://github.com/LyricalString/gale"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def run_cmd(cmd: list[str], cwd: str | None = None, timeout: int = 300) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)


def load_repos() -> list[dict]:
    with open(REPOS_JSON) as f:
        return json.load(f)


# ---------------------------------------------------------------------------
# Differential test (reuse run.py)
# ---------------------------------------------------------------------------


def run_differential_test(repo_name: str, gale_bin: Path) -> dict | None:
    """Run differential test and return report with timing, or None on failure."""
    result = run_cmd(
        [sys.executable, str(SCRIPT_DIR / "run.py"), repo_name,
         "--skip-build", "--gale-bin", str(gale_bin), "--benchmark"],
        cwd=str(GALE_ROOT),
        timeout=600,
    )
    stdout = result.stdout
    print(stdout[-500:] if len(stdout) > 500 else stdout)
    if result.stderr:
        for line in result.stderr.strip().split("\n")[:3]:
            print(f"  [stderr] {line}")

    # Parse the report JSON
    report_file = SCRIPT_DIR / "results" / f"{repo_name}_report.json"
    if not report_file.exists():
        print(f"  [error] No report file found")
        return None

    with open(report_file) as f:
        report = json.load(f)

    # Extract benchmark times from stdout (run.py adds them after saving JSON)
    stylelint_match = re.search(r"Stylelint:\s+([\d.]+)s", stdout)
    gale_match = re.search(r"Gale:\s+([\d.]+)s", stdout)
    if stylelint_match and gale_match:
        report["stylelint_time"] = float(stylelint_match.group(1))
        report["gale_time"] = float(gale_match.group(1))

    return report


# ---------------------------------------------------------------------------
# Package.json modification
# ---------------------------------------------------------------------------


def find_stylelint_in_package_json(pkg: dict) -> dict:
    """Find stylelint references in package.json."""
    info = {
        "has_stylelint_dep": False,
        "stylelint_dep_key": None,  # "devDependencies" or "dependencies"
        "stylelint_version": None,
        "lint_scripts": {},  # script_name -> command
    }

    for dep_key in ["devDependencies", "dependencies"]:
        deps = pkg.get(dep_key, {})
        if "stylelint" in deps:
            info["has_stylelint_dep"] = True
            info["stylelint_dep_key"] = dep_key
            info["stylelint_version"] = deps["stylelint"]
            break

    for name, cmd in pkg.get("scripts", {}).items():
        if "stylelint" in cmd.lower():
            info["lint_scripts"][name] = cmd

    return info


def modify_package_json_source(source: str) -> tuple[str, list[str]]:
    """Modify package.json source text to swap stylelint for gale.

    Works on raw text to preserve formatting, comments, trailing commas, etc.
    Returns (modified_source, list_of_changes).
    """
    changes = []
    result = source

    # Replace "stylelint": "^x.y.z" with "@codebend3r/gale": "^VERSION" in deps
    dep_pattern = r'"stylelint"\s*:\s*"[^"]*"'
    dep_replacement = f'"{GALE_NPM_PACKAGE}": "^{GALE_VERSION}"'
    if re.search(dep_pattern, result):
        result = re.sub(dep_pattern, dep_replacement, result, count=1)
        changes.append("Replaced stylelint dependency with @codebend3r/gale")

    # In scripts: replace "stylelint" command with "gale", and .stylelintcache with .galecache
    # Only replace in script values, not in script names or other fields
    def replace_in_scripts(match):
        line = match.group(0)
        original = line
        # Replace stylelint command (but not config package names like stylelint-config-*)
        line = re.sub(r'\bstylelint\b(?!-config|-scss|-order|-prettier|-standard|-recommended)', 'gale', line)
        line = line.replace('.stylelintcache', '.galecache')
        if line != original:
            changes.append(f"Updated script: {original.strip()[:60]}...")
        return line

    # Find the "scripts" section and replace within it
    scripts_match = re.search(r'"scripts"\s*:\s*\{', result)
    if scripts_match:
        start = scripts_match.start()
        # Find the matching closing brace
        depth = 0
        end = start
        for i in range(scripts_match.end() - 1, len(result)):
            if result[i] == '{':
                depth += 1
            elif result[i] == '}':
                depth -= 1
                if depth == 0:
                    end = i + 1
                    break

        scripts_section = result[start:end]
        # Replace stylelint in each script line
        modified_scripts = re.sub(r'"[^"]*stylelint[^"]*"', replace_in_scripts, scripts_section)
        result = result[:start] + modified_scripts + result[end:]

    return result, changes


# ---------------------------------------------------------------------------
# PR creation
# ---------------------------------------------------------------------------


def generate_pr_body(repo_name: str, stylelint_time: float, gale_time: float) -> str:
    speedup = stylelint_time / gale_time if gale_time > 0 else 999
    return f"""## What this does

I wrote Gale, a Rust rewrite of Stylelint that reads `.stylelintrc.json` directly and produces the same output. This PR swaps it in — the only changes are in package.json, no config changes needed.

To verify: `npx {GALE_NPM_PACKAGE} "**/*.{{css,scss}}"` should produce zero warnings on this repo, same as Stylelint.

## Speed

| Tool | Time |
|------|------|
| Stylelint | {stylelint_time:.2f}s |
| Gale | {gale_time:.2f}s |

Probably not the main bottleneck in CI but it's free.

Source: {GALE_GITHUB}
npm: `{GALE_NPM_PACKAGE}` (darwin-arm64, darwin-x64, linux-arm64, linux-x64)"""


def generate_pr_title(speedup: float) -> str:
    return f"build: replace stylelint with gale (~{speedup:.0f}x faster CSS linting)"


def open_pr_for_repo(
    repo_config: dict,
    gale_bin: Path,
    dry_run: bool = False,
) -> bool:
    """Run the full flow: test, fork, modify, PR. Returns True on success."""
    name = repo_config["name"]
    repo = repo_config["repo"]
    clone_dir = CLONES_DIR / name

    print(f"\n{'='*70}")
    print(f"  {name} ({repo})")
    print(f"{'='*70}")

    if not clone_dir.exists():
        print(f"  [error] Clone not found. Run `python run.py {name}` first.")
        return False

    # Step 1: Run differential test with benchmark
    print(f"\n  [step 1] Running differential test...")
    report = run_differential_test(name, gale_bin)
    if report is None:
        print(f"  [SKIP] Differential test failed")
        return False

    fn = report.get("stylelint_only_warnings", 0)
    fp = report.get("gale_only_warnings", 0)
    stylelint_time = report.get("stylelint_time", 0)
    gale_time = report.get("gale_time", 0)

    if fn > 0 or fp > 0:
        print(f"  [SKIP] Not a clean drop-in: FN={fn}, FP={fp}")
        return False

    if stylelint_time == 0 or gale_time == 0:
        print(f"  [SKIP] No benchmark data (run with --benchmark)")
        return False

    speedup = stylelint_time / gale_time if gale_time > 0 else 999
    print(f"  [OK] 0 FP / 0 FN — Stylelint {stylelint_time:.2f}s, Gale {gale_time:.2f}s ({speedup:.0f}x)")

    # Step 2: Read and modify package.json
    pkg_json_path = clone_dir / "package.json"
    if not pkg_json_path.exists():
        print(f"  [error] No package.json")
        return False

    original_source = pkg_json_path.read_text()
    modified_source, changes = modify_package_json_source(original_source)

    if not changes:
        print(f"  [SKIP] No stylelint references found in package.json")
        return False

    print(f"\n  [step 2] Changes to package.json:")
    for c in changes:
        print(f"    - {c}")

    if dry_run:
        print(f"\n  [DRY RUN] Would open PR with title:")
        print(f"    {generate_pr_title(speedup)}")
        print(f"\n  Diff preview:")
        # Show a simple diff
        for orig_line, mod_line in zip(original_source.splitlines(), modified_source.splitlines()):
            if orig_line != mod_line:
                print(f"    - {orig_line.strip()}")
                print(f"    + {mod_line.strip()}")
        return True

    # Step 3: Fork the repo
    print(f"\n  [step 3] Forking {repo}...")
    fork_result = run_cmd(["gh", "repo", "fork", repo, "--clone=false"], timeout=30)
    if fork_result.returncode != 0 and "already exists" not in fork_result.stderr:
        print(f"  [error] Fork failed: {fork_result.stderr.strip()[:200]}")
        return False

    # Get fork URL
    whoami = run_cmd(["gh", "api", "user", "--jq", ".login"], timeout=10)
    gh_user = whoami.stdout.strip()
    fork_repo = f"{gh_user}/{repo.split('/')[-1]}"
    print(f"  [OK] Fork: {fork_repo}")

    # Step 4: Clone fork to temp dir, apply changes, push
    import tempfile
    with tempfile.TemporaryDirectory() as tmpdir:
        fork_dir = Path(tmpdir) / name
        branch_name = "gale-migration"

        print(f"\n  [step 4] Cloning fork...")
        clone_result = run_cmd(
            ["gh", "repo", "clone", fork_repo, str(fork_dir), "--", "--depth", "1"],
            timeout=120,
        )
        if clone_result.returncode != 0:
            print(f"  [error] Clone fork failed: {clone_result.stderr.strip()[:200]}")
            return False

        # Create branch
        run_cmd(["git", "checkout", "-b", branch_name], cwd=str(fork_dir))

        # Write modified package.json
        (fork_dir / "package.json").write_text(modified_source)

        # Commit
        run_cmd(["git", "add", "package.json"], cwd=str(fork_dir))
        commit_msg = f"build: replace stylelint with gale (~{speedup:.0f}x faster CSS linting)"
        run_cmd(["git", "commit", "-m", commit_msg], cwd=str(fork_dir))

        # Push
        print(f"  [step 5] Pushing to {fork_repo}...")
        push_result = run_cmd(
            ["git", "push", "-u", "origin", branch_name, "--force"],
            cwd=str(fork_dir), timeout=60,
        )
        if push_result.returncode != 0:
            print(f"  [error] Push failed: {push_result.stderr.strip()[:200]}")
            return False

    # Step 5: Open PR
    print(f"\n  [step 6] Opening PR...")
    pr_title = generate_pr_title(speedup)
    pr_body = generate_pr_body(name, stylelint_time, gale_time)

    pr_result = run_cmd(
        ["gh", "pr", "create",
         "--repo", repo,
         "--head", f"{gh_user}:{branch_name}",
         "--title", pr_title,
         "--body", pr_body],
        timeout=30,
    )

    if pr_result.returncode != 0:
        stderr = pr_result.stderr.strip()
        if "already exists" in stderr:
            print(f"  [SKIP] PR already exists")
            return True
        print(f"  [error] PR creation failed: {stderr[:200]}")
        return False

    pr_url = pr_result.stdout.strip()
    print(f"\n  [SUCCESS] PR opened: {pr_url}")
    return True


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main():
    parser = argparse.ArgumentParser(description="Open migration PRs from Stylelint to Gale")
    parser.add_argument("repos", nargs="*", help="Specific repos to PR")
    parser.add_argument("--all", action="store_true", help="PR all repos that pass")
    parser.add_argument("--dry-run", action="store_true", help="Show changes without opening PRs")
    parser.add_argument("--list", action="store_true", help="List available repos")
    parser.add_argument("--skip-build", action="store_true", help="Skip building Gale")
    parser.add_argument("--gale-bin", type=str, help="Path to Gale binary")
    args = parser.parse_args()

    repos = load_repos()

    if args.list:
        print("Available repos:")
        for r in repos:
            print(f"  {r['name']:<20} {r['repo']:<40} {r.get('notes', '')}")
        return

    if not args.all and not args.repos:
        print("Specify repos or use --all")
        sys.exit(1)

    if args.repos:
        repos = [r for r in repos if r["name"] in args.repos]
        if not repos:
            print(f"No repos matched: {args.repos}")
            sys.exit(1)

    # Build or find Gale binary
    if args.gale_bin:
        gale_bin = Path(args.gale_bin)
    elif args.skip_build:
        gale_bin = GALE_ROOT / "target" / "release" / "gale"
    else:
        print("[build] Building Gale...")
        result = run_cmd(["cargo", "build", "--release"], cwd=str(GALE_ROOT), timeout=300)
        if result.returncode != 0:
            print(f"[error] Build failed")
            sys.exit(1)
        gale_bin = GALE_ROOT / "target" / "release" / "gale"

    if not gale_bin.exists():
        print(f"[error] Gale binary not found: {gale_bin}")
        sys.exit(1)

    # Process repos
    success = 0
    skipped = 0
    failed = 0

    for repo_config in repos:
        ok = open_pr_for_repo(repo_config, gale_bin, dry_run=args.dry_run)
        if ok:
            success += 1
        else:
            # Check if it was skipped (not a failure)
            skipped += 1

    print(f"\n{'='*70}")
    print(f"  SUMMARY: {success} PRs opened, {skipped} skipped/failed")
    print(f"{'='*70}")


if __name__ == "__main__":
    main()
