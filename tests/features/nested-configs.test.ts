/**
 * Every file is linted with the rules its own config gives it.
 *
 * The closest config applies to each file, with the overrides in it that
 * match the file; nothing leaks in from the config of the working directory.
 * The same holds for stdin named with `--stdin-filename`, and for the
 * re-lint `--fix` does after each pass.
 */

import { afterAll, describe, expect, test } from "bun:test";

import {
  cleanupProjects,
  config,
  makeProject,
  relativeSource,
  runGale,
  runGaleJson,
} from "./helpers";

afterAll(cleanupProjects);

/** The `rule: text` of each warning, sorted. */
function texts(warnings: { rule: string; text: string }[]): string[] {
  return warnings.map((w) => `${w.rule}: ${w.text}`).sort();
}

describe("a file under a nested config", () => {
  test("gets that config's options, not the root's", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "color-named": ["never", { message: "root says no" }] }),
      "pkg/.stylelintrc.json": config({ "color-named": "never" }),
      "pkg/a.css": "a { color: red; }\n",
    });
    const result = runGaleJson(["pkg/a.css"], { cwd: project.dir });
    expect(texts(result.warnings())).toEqual([
      'color-named: Unexpected named color "red" (color-named)',
    ]);
  });

  test("is ignored by that config's ignoreFiles, relative to it", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "pkg/.stylelintrc.json": config({ "block-no-empty": true }, { ignoreFiles: ["vendor/**"] }),
      "pkg/vendor/a.css": "a {}\n",
      "vendor/b.css": "a {}\n",
    });
    const sources = runGaleJson(["."], { cwd: project.dir })
      .json()
      .map((r) => relativeSource(project.dir, r.source));
    expect(sources).toEqual(["vendor/b.css"]);
  });

  test("is re-linted with that config after each --fix pass", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "length-zero-no-unit": true }),
      "pkg/.stylelintrc.json": config({ "declaration-block-no-duplicate-properties": true }),
      "pkg/a.css": "a { margin: 0px; top: 1px; top: 2px; }\n",
    });
    const result = runGaleJson(["--fix", "pkg/a.css"], { cwd: project.dir });
    expect(result.warnings()).toEqual([]);
    expect(project.read("pkg/a.css")).toBe("a { margin: 0px; top: 2px; }\n");
  });
});

describe("stdin named with --stdin-filename", () => {
  test("is linted with the closest config to that name", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "pkg/.stylelintrc.json": config({ "length-zero-no-unit": true }),
    });
    const result = runGaleJson(["--stdin", "--stdin-filename", "pkg/x.css"], {
      cwd: project.dir,
      stdin: "a {}\nb { margin: 0px; }\n",
    });
    expect(result.warnings().map((w) => w.rule)).toEqual(["length-zero-no-unit"]);
  });
});

describe("an override", () => {
  test("that sets a rule by its deprecated name applies its options", () => {
    const project = makeProject({
      ".stylelintrc.json": config(
        { indentation: 2 },
        { overrides: [{ files: ["**/*.scss"], rules: { indentation: 4 } }] },
      ),
      "a.scss": "a {\n  top: 0;\n}\n",
    });
    const result = runGaleJson(["a.scss"], { cwd: project.dir });
    expect(texts(result.warnings())).toEqual([
      "indentation: Expected indentation of 4 spaces (indentation)",
    ]);
  });

  test("keeps the units an older Stylelint does not know", () => {
    const project = makeProject({
      ".stylelintrc.json": config(
        { "unit-no-unknown": true },
        { overrides: [{ files: ["**/*.scss"], rules: { "block-no-empty": true } }] },
      ),
      "node_modules/stylelint/package.json": JSON.stringify({ version: "14.16.1" }),
      "a.scss": "a { height: 100dvh; }\n",
    });
    const result = runGale(["--formatter", "unix", "a.scss"], { cwd: project.dir });
    expect(result.stdout).toContain('Unexpected unknown unit "dvh" (unit-no-unknown)');
  });
});
