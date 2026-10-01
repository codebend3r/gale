/**
 * What invalidates `--cache`.
 *
 * The cache skips files that were clean last time, so its key must cover
 * everything that can change a file's result: the config that applies to
 * the file (including nested per-directory configs and `overrides`) and the
 * disable-comment report switches, wherever they are set. Otherwise a stale
 * "clean" entry hides new warnings.
 *
 * A cache hit is observed through the debug log line Gale prints for it.
 */

import { chmodSync } from "node:fs";

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale, type SourceResult } from "./helpers";

afterAll(cleanupProjects);

const DEBUG = { GALE_LOG: "debug" };

/** Whether a run reported a cache hit (the debug log may land on either stream). */
function hit(result: { stdout: string; stderr: string }): boolean {
  return /cache hit/i.test(result.stdout + result.stderr);
}

/** Lint with the cache and JSON output, logging cache hits. */
function lint(dir: string, args: string[] = []) {
  return runGale(["--cache", "--formatter", "json", ...args], { cwd: dir, env: DEBUG });
}

/**
 * Rule names reported across the run's JSON output. The debug log shares
 * stdout with the report, so the report is the line holding the JSON array.
 */
function rules(result: ReturnType<typeof lint>): string[] {
  const report = result.stdout.split("\n").find((line) => line.startsWith("["));
  if (report === undefined) {
    throw new Error(`no JSON report.\nstdout: ${result.stdout}\nstderr: ${result.stderr}`);
  }
  const sources = JSON.parse(report) as SourceResult[];
  return sources.flatMap((source) => source.warnings.map((w) => w.rule));
}

const CLEAN = "a { color: red; }\n";

describe("--cache invalidation", () => {
  test("changing a nested directory's config re-lints its files", () => {
    const p = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "sub/.stylelintrc.json": config({ "block-no-empty": true }),
      "sub/a.css": CLEAN,
    });

    const first = lint(p.dir, ["sub/a.css"]);
    expect(first.exitCode).toBe(0);

    p.write("sub/.stylelintrc.json", config({ "color-named": "never" }));
    const second = lint(p.dir, ["sub/a.css"]);

    expect(hit(second)).toBe(false);
    expect(rules(second)).toContain("color-named");
  });

  test("changing an override re-lints the files it matches", () => {
    const withOverride = (rules: Record<string, unknown>) =>
      config({ "block-no-empty": true }, { overrides: [{ files: ["**/*.css"], rules }] });
    const p = makeProject({
      ".stylelintrc.json": withOverride({}),
      "a.css": CLEAN,
    });

    expect(lint(p.dir, ["a.css"]).exitCode).toBe(0);

    p.write(".stylelintrc.json", withOverride({ "color-named": "never" }));
    const second = lint(p.dir, ["a.css"]);

    expect(hit(second)).toBe(false);
    expect(rules(second)).toContain("color-named");
  });

  const DISABLED = "/* stylelint-disable-next-line block-no-empty */\na { color: red; }\n";

  test("turning on a disable report in the config re-lints", () => {
    const p = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": DISABLED,
    });

    expect(lint(p.dir, ["a.css"]).exitCode).toBe(0);

    p.write(
      ".stylelintrc.json",
      config({ "block-no-empty": true }, { reportDescriptionlessDisables: true }),
    );
    const second = lint(p.dir, ["a.css"]);

    expect(hit(second)).toBe(false);
    expect(rules(second)).toContain("--report-descriptionless-disables");
  });

  test("passing a disable-report flag re-lints", () => {
    const p = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": DISABLED,
    });

    expect(lint(p.dir, ["a.css"]).exitCode).toBe(0);

    const second = lint(p.dir, ["--report-descriptionless-disables", "a.css"]);

    expect(hit(second)).toBe(false);
    expect(rules(second)).toContain("--report-descriptionless-disables");
  });

  test("an unchanged nested config still hits", () => {
    const p = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "sub/.stylelintrc.json": config({ "color-named": "never" }),
      "sub/a.css": "a { color: #f00; }\n",
    });

    expect(lint(p.dir, ["sub/a.css"]).exitCode).toBe(0);

    expect(hit(lint(p.dir, ["sub/a.css"]))).toBe(true);
  });

  // Root ignores file permissions, so an unreadable file cannot be staged.
  const canRevokeRead = process.platform !== "win32" && process.getuid?.() !== 0;

  test.skipIf(!canRevokeRead)("metadata: a clean file is skipped without being read", () => {
    const p = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": CLEAN,
    });

    expect(lint(p.dir, ["a.css"]).exitCode).toBe(0);

    // Revoking read access leaves the size and mtime alone, so the metadata
    // fingerprint still matches; a run that reads the file first would
    // fail to and never consult the cache.
    chmodSync(p.path("a.css"), 0o000);
    try {
      expect(hit(lint(p.dir, ["--cache-strategy", "metadata", "a.css"]))).toBe(true);
    } finally {
      chmodSync(p.path("a.css"), 0o644);
    }
  });
});
