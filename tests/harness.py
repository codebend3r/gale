"""
Shared tooling for the test harnesses that run Gale against real repositories.

Loads the repository corpus (tests/differential/repos.json), clones those repos
and installs their dependencies, and builds Gale. The scripts that use it are
run directly rather than as a package, so each puts this directory on sys.path
before importing it:

    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    from harness import ...
"""

import json
import os
import re
import shutil
import subprocess
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
