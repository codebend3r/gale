#!/usr/bin/env python3
"""
Shared tooling for the test harnesses that run Gale against real repositories.

Loads the repository corpus (tests/differential/repos.json), clones those repos
and installs their dependencies, builds Gale, and compares Stylelint's JSON
output with Gale's. The scripts that use it are run directly rather than as a
package, so each puts this directory on sys.path before importing it:

    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    from harness import ...

benchmarks/benchmark.sh runs it as a script instead:

    python3 tests/harness.py repos [NAME...]        # name|cwd|globs|paths
    python3 tests/harness.py prepare NAME --clones-dir DIR
    python3 tests/harness.py compare NAME WORK_DIR STYLELINT_OUT STYLELINT_ERR GALE_OUT GALE_ERR
    python3 tests/harness.py build
"""

import argparse
import contextlib
import json
import os
import re
import shutil
import subprocess
import sys
from collections import Counter
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
GALE_ROOT = TESTS_DIR.parent
GALE_BIN = GALE_ROOT / "target" / "release" / "gale"
REPOS_JSON = TESTS_DIR / "differential" / "repos.json"

# The yarn/pnpm shims are corepack. When a repo pins a package manager version
# corepack has not cached yet, it asks "Do you want to continue? [Y/n]" on
# stdin. Output is captured, so that prompt is invisible and the install hangs
# until it times out. Never prompt; just download.
os.environ.setdefault("COREPACK_ENABLE_DOWNLOAD_PROMPT", "0")


def run_cmd(cmd: list[str], cwd: str | None = None, timeout: int = 300) -> subprocess.CompletedProcess:
    """Run a command and return the result. Does not raise on non-zero exit."""
    return subprocess.run(
        cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout,
    )


def load_repos() -> list[dict]:
    with open(REPOS_JSON) as f:
        return json.load(f)


def repo_globs(repo_config: dict, css_only: bool = False) -> list[str]:
    """The repo's explicit globs, or "{path}/**/*.{css,scss,less}" per path."""
    if repo_config.get("globs"):
        return repo_config["globs"]
    ext_pattern = "*.css" if css_only else "*.{css,scss,less}"
    return [f"{p}/**/{ext_pattern}" for p in repo_config["paths"]]


def build_gale(skip: bool = False) -> Path | None:
    """Build the release binary, or with skip, use the one already built."""
    if skip:
        if not GALE_BIN.exists():
            print("[error] No Gale binary found. Build first or use --gale-bin")
            return None
        print(f"[build] Using existing binary: {GALE_BIN}")
        return GALE_BIN

    print("[build] Building Gale (release)...")
    result = run_cmd(["cargo", "build", "--release"], cwd=str(GALE_ROOT), timeout=1800)
    if result.returncode != 0:
        print(f"[error] Gale build failed: {result.stderr.strip()[:500]}")
        return None

    if not GALE_BIN.exists():
        print("[error] Gale binary not found after build")
        return None

    print("[build] Gale binary ready")
    return GALE_BIN


# ---------------------------------------------------------------------------
# Cloning and installing
# ---------------------------------------------------------------------------


def clone_repo(repo: str, branch: str, dest: Path, pull: bool = False, force: bool = False) -> bool:
    """Shallow-clone a repo. Returns True if the clone dir exists after.

    An existing clone is kept as is, or with pull, fast-forwarded. With force
    it is deleted and cloned again.
    """
    if dest.exists() and not force:
        if not pull:
            print(f"  [skip] Already cloned: {dest.name}")
            return True
        print(f"  [pull] Updating existing clone: {dest.name}")
        result = run_cmd(["git", "pull", "--ff-only"], cwd=str(dest), timeout=60)
        if result.returncode != 0:
            print(f"  [warn] git pull failed (continuing with existing clone): {result.stderr.strip()[:200]}")
        return True

    if dest.exists():
        shutil.rmtree(dest)

    print(f"  [clone] {repo} @ {branch} -> {dest.name}")
    result = run_cmd(
        ["git", "clone", "--depth", "1", "--branch", branch,
         f"https://github.com/{repo}.git", str(dest)],
        timeout=120,
    )
    if result.returncode != 0:
        print(f"  [error] Clone failed: {result.stderr.strip()[:200]}")
        shutil.rmtree(dest, ignore_errors=True)
        return False
    return True


def find_up(start: Path, stop: Path, target: str) -> Path | None:
    """The nearest directory from start up to stop that contains target."""
    current = start
    while True:
        if (current / target).exists():
            return current
        if current == stop or current == current.parent:
            return None
        current = current.parent


def detect_package_manager(install_dir: Path) -> str:
    """Pick the installer for a cloned repo.

    bun wherever it can honour the repo's own lockfile: bun.lock,
    package-lock.json (bun migrates it), and Yarn v1 lockfiles. pnpm and Yarn
    Berry lockfiles are not migratable, and bun would silently resolve fresh
    and drift from the pinned Stylelint version, so those repos use their own
    tool.
    """
    if (install_dir / "pnpm-lock.yaml").exists():
        return "pnpm"
    yarn_lock = install_dir / "yarn.lock"
    if yarn_lock.exists() and re.search(r"^__metadata:", yarn_lock.read_text(errors="replace"), re.M):
        return "yarn"
    return "bun"


INSTALL_COMMANDS = {
    "bun": ["bun", "install", "--ignore-scripts"],
    "pnpm": ["pnpm", "install", "--ignore-scripts", "--no-frozen-lockfile"],
    "yarn": ["yarn", "install", "--mode", "skip-build"],
    "npm": ["npm", "install", "--ignore-scripts"],
}


def _put_bun_shims_on_path():
    """bun's global bin dir holds the corepack shims (yarn/pnpm) that Yarn
    Berry and pnpm repos need, and it is frequently not on PATH."""
    if not shutil.which("bun"):
        return
    result = run_cmd(["bun", "pm", "bin", "-g"], timeout=10)
    bun_bin = result.stdout.strip()
    path = os.environ.get("PATH", "")
    if bun_bin and Path(bun_bin).is_dir() and bun_bin not in path.split(os.pathsep):
        os.environ["PATH"] = bun_bin + os.pathsep + path


def _get_current_node_version() -> str:
    """Return the current Node.js version string (e.g., '22.21.1')."""
    result = run_cmd(["node", "--version"], timeout=10)
    return result.stdout.strip().lstrip("v")


def _check_node_version(install_dir: Path) -> tuple[bool, str | None]:
    """Check if the repo requires a Node version we don't have.

    Returns (ok, required_version). If ok is False, required_version is the
    version the repo needs.
    """
    nvmrc = install_dir / ".nvmrc"
    node_version_file = install_dir / ".node-version"

    required = None
    if nvmrc.exists():
        required = nvmrc.read_text().strip().lstrip("v")
    elif node_version_file.exists():
        required = node_version_file.read_text().strip().lstrip("v")

    if not required:
        return True, None

    current = _get_current_node_version()

    # Compare major.minor.patch — require exact match or current >= required
    try:
        req_parts = [int(x) for x in required.split(".")]
        cur_parts = [int(x) for x in current.split(".")]
        if cur_parts >= req_parts:
            return True, None
        return False, required
    except ValueError:
        # Can't parse, assume it's fine
        return True, None


def install_deps(
    clone_dir: Path,
    work_dir: Path | None = None,
    search_paths: list[str] | None = None,
) -> bool | str:
    """Install npm dependencies (needed for Stylelint and its plugins).

    Installs from the nearest package.json at or above work_dir (the clone
    root by default), or failing that, in the first search path that has one.

    Returns True on success, False on hard failure, or "skip" if the repo
    should be skipped (e.g., wrong Node version).
    """
    # Monorepos: package.json may live below the clone root.
    install_dir = find_up(work_dir or clone_dir, clone_dir, "package.json")
    if install_dir is None and search_paths:
        for sp in search_paths:
            if (clone_dir / sp / "package.json").exists():
                install_dir = clone_dir / sp
                print(f"  [info] Found package.json in subdirectory: {sp}/")
                break
    if install_dir is None:
        print("  [warn] No package.json found")
        return False

    if (install_dir / "node_modules").exists():
        print("  [skip] node_modules already exists")
        return True

    # Check Node.js version requirement before attempting install.
    # Check both the clone root and install dir (for monorepos).
    for check_dir in dict.fromkeys([clone_dir, install_dir]):
        ok, required_version = _check_node_version(check_dir)
        if not ok:
            current = _get_current_node_version()
            print(f"  [skip] requires Node {required_version} (current: {current})")
            return "skip"

    pm = detect_package_manager(install_dir)
    print(f"  [install] Installing dependencies with {pm}...")

    _put_bun_shims_on_path()
    run_cmd(["corepack", "enable"], cwd=str(install_dir), timeout=30)

    # Yarn Berry defaults to PnP, which leaves no node_modules/.bin/stylelint.
    yarnrc = install_dir / ".yarnrc.yml"
    if pm == "yarn" and yarnrc.exists() and "nodeLinker" not in yarnrc.read_text():
        with open(yarnrc, "a") as f:
            f.write("\nnodeLinker: node-modules\n")

    result = run_cmd(INSTALL_COMMANDS[pm], cwd=str(install_dir), timeout=300)

    # Some npm lockfiles fail bun's integrity check (Mattermost, whose lockfile
    # sits at the workspace root above the install dir). npm is the only tool
    # that can honour those, so fall back to it rather than skip the repo.
    if result.returncode != 0 and pm == "bun" and shutil.which("npm"):
        print(f"  [warn] bun install failed (exit {result.returncode}); retrying with npm")
        shutil.rmtree(install_dir / "node_modules", ignore_errors=True)
        pm = "npm"
        result = run_cmd(INSTALL_COMMANDS[pm], cwd=str(install_dir), timeout=300)

    if result.returncode != 0:
        print(f"  [error] {pm} install failed: {result.stderr.strip()[:200]}")
        return False
    return True


def find_stylelint_bin(work_dir: Path, clone_dir: Path, search_paths: list[str] | None = None) -> Path | None:
    """Find the repo's Stylelint binary.

    Workspaces hoist it above the work dir, so look from there up to the clone
    root, then in the search paths (monorepos installed in a subdirectory).
    """
    found = find_up(work_dir, clone_dir, "node_modules/.bin/stylelint")
    if found:
        return found / "node_modules" / ".bin" / "stylelint"
    for sp in search_paths or []:
        candidate = clone_dir / sp / "node_modules" / ".bin" / "stylelint"
        if candidate.exists():
            return candidate
    return None


# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------


def parse_report(*texts: str) -> list[dict] | None:
    """Return the JSON report from whichever of texts holds it.

    Stylelint 16+ writes the report to stderr, where Node also prints its own
    warnings, e.g. "(node:123) [DEP0040] DeprecationWarning: ...". A bracket in
    such a line is not the report, so try each `[` in turn until one decodes to
    a list of per-file results. Returns None if none does.
    """
    decoder = json.JSONDecoder()
    for text in texts:
        start = text.find("[")
        while start != -1:
            try:
                data, _ = decoder.raw_decode(text, start)
            except json.JSONDecodeError:
                data = None
            if isinstance(data, list) and all(
                isinstance(entry, dict) and "warnings" in entry for entry in data
            ):
                return data
            start = text.find("[", start + 1)
    return None


def normalize_results(results: list[dict], work_dir: Path) -> dict[str, list[dict]]:
    """Normalize linter output into a comparable structure.

    Returns: { relative_path: [sorted list of {line, column, rule, severity, text}] }
    """
    # Stylelint reports absolute source paths and Gale reports paths relative
    # to the working directory; compare both relative to it or nothing matches.
    work_dir = Path(work_dir).resolve()
    normalized = {}

    for entry in results:
        source = entry.get("source", "")
        rel_path = os.path.relpath(source, work_dir) if os.path.isabs(source) else source

        warnings = []
        for w in entry.get("warnings", []):
            rule = w.get("rule")
            # Stylelint appends " (rule-name)" to every message; strip it so
            # comparisons focus on the actual message content.
            text = w.get("text", "")
            if rule and text.endswith(f" ({rule})"):
                text = text[: -(len(rule) + 3)]
            warnings.append({
                "line": w.get("line"),
                "column": w.get("column"),
                "rule": rule,
                "severity": w.get("severity"),
                "text": text,
            })

        warnings.sort(key=lambda w: (w["line"] or 0, w["column"] or 0, w["rule"] or ""))
        normalized[rel_path] = warnings

    return normalized


def compare_results(
    stylelint: dict[str, list[dict]],
    gale: dict[str, list[dict]],
) -> dict:
    """Compare normalized results and return a report."""
    all_files = sorted(set(list(stylelint.keys()) + list(gale.keys())))

    report = {
        "total_files": len(all_files),
        "files_match": 0,
        "files_differ": 0,
        "stylelint_only_warnings": 0,
        "gale_only_warnings": 0,
        "matching_warnings": 0,
        "diffs": [],
    }

    for file_path in all_files:
        s_warnings = stylelint.get(file_path, [])
        g_warnings = gale.get(file_path, [])

        # Use Counter (multiset) to detect duplicate warnings correctly.
        # If Gale emits a warning twice and Stylelint once, the extra is a FP.
        s_counter = Counter(_warning_key(w) for w in s_warnings)
        g_counter = Counter(_warning_key(w) for w in g_warnings)

        matching = sum((s_counter & g_counter).values())
        stylelint_only_counter = s_counter - g_counter
        gale_only_counter = g_counter - s_counter
        stylelint_only = set(stylelint_only_counter.elements())
        gale_only = set(gale_only_counter.elements())

        report["matching_warnings"] += matching
        report["stylelint_only_warnings"] += sum(stylelint_only_counter.values())
        report["gale_only_warnings"] += sum(gale_only_counter.values())

        if stylelint_only or gale_only:
            report["files_differ"] += 1
            report["diffs"].append({
                "file": file_path,
                "matching": matching,
                "stylelint_only": [_key_to_dict(k) for k in sorted(stylelint_only)],
                "gale_only": [_key_to_dict(k) for k in sorted(gale_only)],
            })
        else:
            report["files_match"] += 1

    return report


def _warning_key(w: dict) -> tuple:
    return (w["line"], w["column"], w["rule"], w["severity"], w["text"])


def _key_to_dict(key: tuple) -> dict:
    return {"line": key[0], "column": key[1], "rule": key[2],
            "severity": key[3], "text": key[4]}


def exclude_expected_diffs(report: dict, expected_diffs: list[dict]) -> int:
    """Drop the repo's expected diffs from report; returns how many it dropped."""
    expected_keys = {
        (d["file"], d["line"], d["column"], d["rule"])
        for d in expected_diffs
    }
    filtered_diffs = []
    excluded = 0
    for diff_entry in report["diffs"]:
        new_s_only = [
            w for w in diff_entry["stylelint_only"]
            if (diff_entry["file"], w["line"], w["column"], w["rule"]) not in expected_keys
        ]
        new_g_only = [
            w for w in diff_entry["gale_only"]
            if (diff_entry["file"], w["line"], w["column"], w["rule"]) not in expected_keys
        ]
        s_excluded = len(diff_entry["stylelint_only"]) - len(new_s_only)
        g_excluded = len(diff_entry["gale_only"]) - len(new_g_only)
        excluded += s_excluded + g_excluded
        report["stylelint_only_warnings"] -= s_excluded
        report["gale_only_warnings"] -= g_excluded
        report["matching_warnings"] += min(s_excluded, g_excluded)
        if new_s_only or new_g_only:
            diff_entry["stylelint_only"] = new_s_only
            diff_entry["gale_only"] = new_g_only
            filtered_diffs.append(diff_entry)
        else:
            report["files_differ"] -= 1
            report["files_match"] += 1
    report["diffs"] = filtered_diffs
    return excluded


def compare(
    stylelint_results: list[dict],
    gale_results: list[dict],
    work_dir: Path,
    expected_diffs: list[dict] | None = None,
) -> tuple[dict, int]:
    """Compare both linters' raw output with no rule filters on either side.

    Returns the report and how many expected diffs were excluded from it.
    """
    report = compare_results(
        normalize_results(stylelint_results, work_dir),
        normalize_results(gale_results, work_dir),
    )
    excluded = exclude_expected_diffs(report, expected_diffs) if expected_diffs else 0
    return report, excluded


# ---------------------------------------------------------------------------
# Command line (benchmarks/benchmark.sh)
# ---------------------------------------------------------------------------


def _repo_config(name: str) -> dict:
    for repo in load_repos():
        if repo["name"] == name:
            return repo
    sys.exit(f"Unknown repo: {name}")


def cmd_repos(args) -> int:
    """Print name|cwd|globs|paths per repo, its globs and paths space-separated."""
    repos = {r["name"]: r for r in load_repos() if not r.get("skip")}
    unknown = [n for n in args.names if n not in repos]
    if unknown:
        print(f"No repos matched: {', '.join(unknown)}. Available: {', '.join(repos)}", file=sys.stderr)
        return 1
    for name in args.names or repos:
        repo = repos[name]
        print("|".join([name, repo.get("cwd", ""), " ".join(repo_globs(repo)), " ".join(repo["paths"])]))
    return 0


def cmd_prepare(args) -> int:
    """Clone and install one repo, then print the path of its Stylelint."""
    repo = _repo_config(args.name)
    clone_dir = Path(args.clones_dir) / repo["name"]
    work_dir = clone_dir / repo["cwd"] if repo.get("cwd") else clone_dir
    # Progress goes to stderr; stdout carries only the binary's path.
    with contextlib.redirect_stdout(sys.stderr):
        if not clone_repo(repo["repo"], repo["branch"], clone_dir):
            return 1
        if install_deps(clone_dir, work_dir=work_dir, search_paths=repo["paths"]) is not True:
            return 1
        stylelint_bin = find_stylelint_bin(work_dir, clone_dir, repo["paths"])
        if stylelint_bin is None:
            print("  [error] Stylelint binary not found")
            return 1
    print(stylelint_bin)
    return 0


def cmd_compare(args) -> int:
    """Compare two saved runs; print files|false positives|false negatives."""
    repo = _repo_config(args.name)
    results = {}
    for linter, paths in (("Stylelint", args.stylelint), ("Gale", args.gale)):
        texts = [Path(p).read_text(errors="replace") for p in paths if Path(p).exists()]
        results[linter] = parse_report(*texts)
        if results[linter] is None:
            # Without a report every warning of the other linter would count
            # as a mismatch; fail the comparison instead.
            print(f"no JSON report from {linter}", file=sys.stderr)
            return 3
    report, _ = compare(results["Stylelint"], results["Gale"], Path(args.work_dir), repo.get("expected_diffs"))
    print(f"{report['total_files']}|{report['gale_only_warnings']}|{report['stylelint_only_warnings']}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Shared harness for the repo corpus")
    commands = parser.add_subparsers(dest="command", required=True)

    repos = commands.add_parser("repos", help="List the repos that are not skipped")
    repos.add_argument("names", nargs="*", help="Only these repos, in this order")
    repos.set_defaults(func=cmd_repos)

    prepare = commands.add_parser("prepare", help="Clone and install a repo; print its Stylelint")
    prepare.add_argument("name")
    prepare.add_argument("--clones-dir", required=True)
    prepare.set_defaults(func=cmd_prepare)

    compare_cmd = commands.add_parser("compare", help="Compare saved Stylelint and Gale JSON output")
    compare_cmd.add_argument("name")
    compare_cmd.add_argument("work_dir", help="The directory both linters ran from")
    compare_cmd.add_argument("stylelint", nargs=2, metavar=("STYLELINT_OUT", "STYLELINT_ERR"))
    compare_cmd.add_argument("gale", nargs=2, metavar=("GALE_OUT", "GALE_ERR"))
    compare_cmd.set_defaults(func=cmd_compare)

    build = commands.add_parser("build", help="Build the release binary")
    build.set_defaults(func=lambda args: 0 if build_gale() else 1)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
