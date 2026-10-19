#!/usr/bin/env python3
"""
Generate COMPATIBILITY.md from differential test results.

Reads the `<name>_report.json` files that tests/differential/run.py writes
(with --benchmark, so they carry both linters' times) and takes each repo's
stars from tests/differential/repos.json. Every repo marked `weekly` there gets
a row; one without a report is shown as a failed run.

Usage: python3 scripts/generate-compatibility-matrix.py results/
"""

import json
import sys
from datetime import datetime, timezone
from pathlib import Path

REPOS_JSON = Path(__file__).resolve().parent.parent / "tests" / "differential" / "repos.json"


def load_report(path: Path) -> dict | None:
    """Return the report, or None when the run left no usable one.

    A run that crashed, timed out, or never linted anything does NOT produce a
    report over a non-empty file set. Rendering that as "0 FP, 0 FN" would make
    a broken run indistinguishable from perfect parity, so the row is marked
    failed instead.
    """
    try:
        report = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return None
    if report.get("timeout") or not report.get("total_files"):
        return None
    return report


def main():
    results_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("results")

    repos = {r["name"]: r for r in json.loads(REPOS_JSON.read_text())}
    names = {name for name, r in repos.items() if r.get("weekly")}
    if results_dir.is_dir():
        names |= {p.name.removesuffix("_report.json") for p in results_dir.glob("*_report.json")}

    rows = []
    for name in sorted(names):
        meta = repos.get(name, {})
        full = meta.get("repo", name)
        stars = meta.get("stars", "?")
        report = load_report(results_dir / f"{name}_report.json")

        if report is None:
            rows.append({
                "full": full,
                "stars": stars,
                "files": "—",
                "pct": "**run failed**",
                "fp": "—",
                "fn": "—",
                "speedup": "—",
                "ok": False,
            })
            continue

        total = report["total_files"]
        match = report["files_match"]
        s_time = report.get("stylelint_time")
        g_time = report.get("gale_time")
        speedup = f"{s_time / g_time:.1f}" if s_time and g_time else "?"

        rows.append({
            "full": full,
            "stars": stars,
            "files": f"{match}/{total}",
            "pct": f"{match/total*100:.0f}%",
            "fp": report["gale_only_warnings"],
            "fn": report["stylelint_only_warnings"],
            "speedup": speedup,
            "ok": True,
        })

    now = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    print("# Compatibility Matrix")
    print()
    print(f"Last updated: {now}")
    print()
    print("Gale is tested weekly against popular open-source repositories that use Stylelint.")
    print("Both tools run on the same files with the same config. Results are compared automatically.")
    print()
    print("| Repository | Stars | Files | Pass | FP | FN | Speedup |")
    print("|------------|-------|-------|------|----|----|---------|")

    for r in rows:
        repo_link = f"[{r['full']}](https://github.com/{r['full']})"
        speedup = f"{r['speedup']}x" if r["ok"] else r["speedup"]
        print(f"| {repo_link} | {r['stars']} | {r['files']} | {r['pct']} | {r['fp']} | {r['fn']} | {speedup} |")

    failed = [r["full"] for r in rows if not r["ok"]]
    if failed:
        print()
        print(
            f"> **{len(failed)} run(s) did not complete** and are shown as "
            f"`run failed`: {', '.join(failed)}. A failed run is not evidence "
            "of parity — check the workflow logs."
        )

    print()
    print("### Legend")
    print()
    print("- **Files**: Matching files / Total files analyzed")
    print("- **Pass**: Percentage of files where Gale and Stylelint produce identical output")
    print("- **FP**: False positives — warnings Gale reports but Stylelint does not")
    print("- **FN**: False negatives — warnings Stylelint reports but Gale misses")
    print("- **Speedup**: How many times faster Gale is compared to Stylelint")
    print("- **run failed**: The differential run produced no comparable output; the row carries no parity information")
    print()
    print("### How to reproduce")
    print()
    print("```bash")
    print("git clone https://github.com/codebend3r/gale && cd gale")
    print("cargo build --release")
    print("python3 tests/differential/run.py bootstrap --benchmark")
    print("```")
    print()
    print("See [benchmarks/](benchmarks/) for the full benchmark script.")


if __name__ == "__main__":
    main()
