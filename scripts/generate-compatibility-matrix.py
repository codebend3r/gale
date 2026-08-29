#!/usr/bin/env python3
"""
Generate COMPATIBILITY.md from differential test results.

Usage: python3 scripts/generate-compatibility-matrix.py results/
"""

import re
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO_META = {
    "bootstrap": {"full": "twbs/bootstrap", "stars": "168K", "description": "The most popular CSS framework"},
    "gutenberg": {"full": "wordpress/gutenberg", "stars": "10K", "description": "WordPress block editor"},
    "grafana": {"full": "grafana/grafana", "stars": "62K", "description": "Observability platform"},
    "primer-css": {"full": "primer/css", "stars": "12K", "description": "GitHub's design system"},
    "carbon": {"full": "carbon-design-system/carbon", "stars": "7K", "description": "IBM's design system"},
    "material-ui": {"full": "mui/material-ui", "stars": "94K", "description": "React UI library"},
}


def parse_result(text: str) -> dict:
    """Extract metrics from a differential test result.

    A run that crashed, timed out, or never linted anything does NOT produce
    these lines.  Defaulting the counts to 0 in that case would render a broken
    run as "0 FP, 0 FN" — indistinguishable from perfect parity.  Missing
    metrics are therefore reported as `None` and the row is marked failed.
    """
    metrics = {}

    def number(pattern, cast=int):
        m = re.search(pattern, text)
        return cast(m.group(1)) if m else None

    metrics["files_total"] = number(r"Files analyzed:\s+(\d+)")
    metrics["files_match"] = number(r"Files matching:\s+(\d+)")
    metrics["fp"] = number(r"Gale-only \(FP\):\s+(\d+)")
    metrics["fn"] = number(r"Stylelint-only \(FN\):\s+(\d+)")
    metrics["speedup"] = number(r"Speedup:\s+([\d.]+)x", str)
    metrics["gale_time"] = number(r"Gale:\s+([\d.]+)s", str)
    metrics["stylelint_time"] = number(r"Stylelint:\s+([\d.]+)s", str)

    # A usable run reports both comparison counts over a non-empty file set.
    metrics["ok"] = (
        metrics["fp"] is not None
        and metrics["fn"] is not None
        and metrics["files_total"] is not None
        and metrics["files_total"] > 0
    )
    return metrics


def main():
    results_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("results")

    rows = []
    for result_dir in sorted(results_dir.iterdir()):
        if not result_dir.is_dir():
            continue

        name = result_dir.name.replace("result-", "")
        result_file = result_dir / "result.txt"
        if not result_file.exists():
            continue

        text = result_file.read_text()
        metrics = parse_result(text)
        meta = REPO_META.get(name, {"full": name, "stars": "?", "description": ""})

        if not metrics["ok"]:
            rows.append({
                "name": name,
                "full": meta["full"],
                "stars": meta["stars"],
                "description": meta["description"],
                "files": "—",
                "pct": "**run failed**",
                "fp": "—",
                "fn": "—",
                "speedup": "—",
                "ok": False,
            })
            continue

        total = metrics["files_total"]
        match = metrics["files_match"] or 0
        pct = f"{match/total*100:.0f}%"

        rows.append({
            "name": name,
            "full": meta["full"],
            "stars": meta["stars"],
            "description": meta["description"],
            "files": f"{match}/{total}",
            "pct": pct,
            "fp": metrics["fp"],
            "fn": metrics["fn"],
            "speedup": metrics["speedup"] or "?",
            "gale_time": metrics["gale_time"] or "?",
            "stylelint_time": metrics["stylelint_time"] or "?",
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
    print("git clone https://github.com/user/gale && cd gale")
    print("cargo build --release")
    print("python3 tests/differential/run.py bootstrap --benchmark")
    print("```")
    print()
    print("See [benchmarks/](benchmarks/) for the full benchmark script.")


if __name__ == "__main__":
    main()
