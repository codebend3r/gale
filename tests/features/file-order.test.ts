/**
 * The order files appear in the report.
 *
 * Stylelint reports files in the order its glob lists them: arguments in
 * the order given, each walked breadth first (a directory's own files before
 * its subdirectories'), and each file once however many arguments match it.
 * Gale walks directories in parallel, so it sorts each walk the same way to
 * report the same order on every run.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, relativeSource, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

function project() {
  const files: Record<string, string> = {
    ".stylelintrc.json": config({ "block-no-empty": true }),
  };
  for (const path of [
    "z.css",
    "m.css",
    "a/x.css",
    "a/b.css",
    "a/b/deep.css",
    "a/b/c/deeper.css",
    "b/y.css",
    "c/d/e/f/g.css",
  ]) {
    files[path] = "a {}\n";
  }
  return makeProject(files);
}

/**
 * The `source` of every result, in report order, relative to the project
 * (gale reports absolute paths, as Stylelint does).
 */
function sources(args: string[], dir: string): string[] {
  return runGaleJson(args, { cwd: dir })
    .json()
    .map((result) => relativeSource(dir, result.source));
}

describe("report order", () => {
  test("a glob is reported breadth first, the same on every run", () => {
    const { dir } = project();
    const expected = [
      "m.css",
      "z.css",
      "a/b.css",
      "a/x.css",
      "b/y.css",
      "a/b/deep.css",
      "a/b/c/deeper.css",
      "c/d/e/f/g.css",
    ];

    for (let run = 0; run < 5; run++) {
      expect(sources(["**/*.css"], dir)).toEqual(expected);
    }
  });

  test("arguments keep their order and a file is reported once", () => {
    const { dir } = project();

    expect(sources(["z.css", "a", "**/*.css", "z.css"], dir)).toEqual([
      "z.css",
      "a/b.css",
      "a/x.css",
      "a/b/deep.css",
      "a/b/c/deeper.css",
      "m.css",
      "b/y.css",
      "c/d/e/f/g.css",
    ]);
  });
});
