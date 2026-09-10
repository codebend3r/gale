/**
 * Stylelint CLI flags Gale does not accept yet.
 *
 * Tier 1.
 */

import { afterAll, describe, expect, test } from "bun:test";

import {
  EMPTY_BLOCK,
  cleanupProjects,
  config,
  hasAnsi,
  makeProject,
  runGale,
  runGaleJson,
} from "./helpers";

afterAll(cleanupProjects);

const RULES = { "block-no-empty": true };

/** Normalise the `source` paths Gale prints for a directory walk. */
function sources(result: ReturnType<typeof runGaleJson>): string[] {
  return result
    .json()
    .map((r) => r.source.replace(/^\.\//, ""))
    .sort();
}

describe("[tier 1] --ignore-pattern", () => {
  const files = {
    ".stylelintrc.json": config(RULES),
    "a.css": EMPTY_BLOCK,
    "vendor/b.css": EMPTY_BLOCK,
    "vendor/deep/c.css": EMPTY_BLOCK,
  };

  test("every file is linted without the flag", () => {
    const project = makeProject(files);

    expect(sources(runGaleJson(["**/*.css"], { cwd: project.dir }))).toEqual([
      "a.css",
      "vendor/b.css",
      "vendor/deep/c.css",
    ]);
  });

  test("a glob excludes matching files", () => {
    const project = makeProject(files);

    const result = runGaleJson(["**/*.css", "--ignore-pattern", "vendor/**"], {
      cwd: project.dir,
    });

    expect(sources(result)).toEqual(["a.css"]);
  });

  test("the flag can be repeated", () => {
    const project = makeProject(files);

    const result = runGaleJson(
      ["**/*.css", "--ignore-pattern", "vendor/b.css", "--ignore-pattern", "vendor/deep/**"],
      { cwd: project.dir },
    );

    expect(sources(result)).toEqual(["a.css"]);
  });

  test("--ip is the short alias", () => {
    const project = makeProject(files);

    const result = runGaleJson(["**/*.css", "--ip", "vendor/**"], { cwd: project.dir });

    expect(sources(result)).toEqual(["a.css"]);
  });
});

describe("[tier 1] --disable-default-ignores", () => {
  const files = {
    ".stylelintrc.json": config(RULES),
    "a.css": "a { color: red; }\n",
    "node_modules/pkg/x.css": EMPTY_BLOCK,
  };

  test("node_modules is skipped by default", () => {
    const project = makeProject(files);

    expect(sources(runGaleJson(["**/*.css"], { cwd: project.dir }))).toEqual(["a.css"]);
  });

  test("the flag lints node_modules too", () => {
    const project = makeProject(files);

    const result = runGaleJson(["**/*.css", "--disable-default-ignores"], { cwd: project.dir });

    expect(sources(result)).toEqual(["a.css", "node_modules/pkg/x.css"]);
  });

  test("--di is the short alias", () => {
    const project = makeProject(files);

    const result = runGaleJson(["**/*.css", "--di"], { cwd: project.dir });

    expect(sources(result)).toEqual(["a.css", "node_modules/pkg/x.css"]);
  });
});

describe("[tier 1] --quiet-deprecation-warnings", () => {
  test("the flag is accepted", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "a.css": "a { color: red; }\n",
    });

    const result = runGale(["--quiet-deprecation-warnings", "a.css"], { cwd: project.dir });

    expect(result.stderr).not.toMatch(/unexpected argument/i);
    expect(result.exitCode).toBe(0);
  });
});

describe("[tier 1] --custom-syntax", () => {
  // Plain CSS parsing ignores the `$c` declaration, so the SCSS rule only fires
  // when the file is parsed as SCSS.
  const files = {
    ".stylelintrc.json": config({ "scss/dollar-variable-pattern": "^foo" }),
    "a.css": "$c: red;\na { color: $c; }\n",
  };

  test("a .css file is parsed as CSS by default", () => {
    const project = makeProject(files);

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test("postcss-scss parses every file as SCSS", () => {
    const project = makeProject(files);

    const warnings = runGaleJson(["--custom-syntax", "postcss-scss", "a.css"], {
      cwd: project.dir,
    }).warnings();

    expect(warnings.map((w) => w.rule)).toEqual(["scss/dollar-variable-pattern"]);
  });

  test("postcss-less parses every file as Less", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": ".mixin() { color: red; }\n.a { .mixin(); }\nb {}\n",
    });

    const warnings = runGaleJson(["--custom-syntax", "postcss-less", "a.css"], {
      cwd: project.dir,
    }).warnings();

    expect(warnings.map((w) => w.rule)).toEqual(["block-no-empty"]);
  });

  test("an unsupported syntax skips every file instead of failing", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "a.css": EMPTY_BLOCK,
    });

    const result = runGaleJson(["--custom-syntax", "postcss-markdown", "a.css"], {
      cwd: project.dir,
    });

    expect(result.exitCode).toBe(0);
    expect(result.json()).toEqual([]);
  });
});

describe("[tier 1] --output-file", () => {
  test("writes the report to the given path", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": EMPTY_BLOCK });

    runGale(["a.css", "--output-file", "report.txt"], { cwd: project.dir });

    expect(project.exists("report.txt")).toBe(true);
    expect(project.read("report.txt")).toContain("block-no-empty");
  });

  test("creates missing parent directories", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": EMPTY_BLOCK });

    runGale(["a.css", "--output-file", "out/nested/report.txt"], { cwd: project.dir });

    expect(project.read("out/nested/report.txt")).toContain("block-no-empty");
  });

  test("strips ANSI colour codes from the file", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": EMPTY_BLOCK });

    const result = runGale(["a.css", "--output-file", "report.txt"], {
      cwd: project.dir,
      env: { FORCE_COLOR: "1" },
    });

    expect(hasAnsi(result.stdout)).toBe(true);
    expect(hasAnsi(project.read("report.txt"))).toBe(false);
  });

  test("still prints the report to the terminal", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": EMPTY_BLOCK });

    const result = runGale(["a.css", "--output-file", "report.txt"], { cwd: project.dir });

    expect(result.stdout).toContain("block-no-empty");
  });

  test("works with the json formatter", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": EMPTY_BLOCK });

    runGale(["a.css", "--formatter", "json", "--output-file", "report.json"], {
      cwd: project.dir,
    });

    const parsed = JSON.parse(project.read("report.json")) as { warnings: unknown[] }[];
    expect(parsed[0].warnings).toHaveLength(1);
  });

  test("-o is the short alias", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": EMPTY_BLOCK });

    runGale(["a.css", "-o", "report.txt"], { cwd: project.dir });

    expect(project.read("report.txt")).toContain("block-no-empty");
  });
});
