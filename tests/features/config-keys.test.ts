/**
 * Stylelint lets every CLI switch also be set from the config file. These
 * tests cover the config keys Gale accepts only as CLI flags today.
 *
 * Tier 1.
 */

import { afterAll, describe, expect, test } from "bun:test";

import {
  EMPTY_BLOCK,
  UPPER_HEX,
  cleanupProjects,
  config,
  makeProject,
  runGale,
  runGaleJson,
} from "./helpers";

afterAll(cleanupProjects);

describe("[tier 1] ignoreDisables config key", () => {
  const source = "/* stylelint-disable block-no-empty */\na {}\n";

  test("disable comments apply when the key is absent", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": source,
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test("the --ignore-disables flag reports the suppressed warning", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": source,
    });

    const warnings = runGaleJson(["--ignore-disables", "a.css"], { cwd: project.dir }).warnings();
    expect(warnings.map((w) => w.rule)).toEqual(["block-no-empty"]);
  });

  test.failing("ignoreDisables: true reports the suppressed warning", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { ignoreDisables: true }),
      "a.css": source,
    });

    const warnings = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect(warnings.map((w) => w.rule)).toEqual(["block-no-empty"]);
  });

  test("ignoreDisables: false keeps the default behaviour", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { ignoreDisables: false }),
      "a.css": source,
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });
});

describe("[tier 1] allowEmptyInput config key", () => {
  test("no matching files is an error by default", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
    });

    expect(runGale(["missing/**/*.css"], { cwd: project.dir }).exitCode).not.toBe(0);
  });

  test("the --allow-empty-input flag makes no matches succeed", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
    });

    expect(
      runGale(["--allow-empty-input", "missing/**/*.css"], { cwd: project.dir }).exitCode,
    ).toBe(0);
  });

  test.failing("allowEmptyInput: true makes no matches succeed", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { allowEmptyInput: true }),
    });

    expect(runGale(["missing/**/*.css"], { cwd: project.dir }).exitCode).toBe(0);
  });
});

describe("[tier 1] quiet config key", () => {
  const rules = { "block-no-empty": [true, { severity: "warning" }] };

  test("warnings are reported when the key is absent", () => {
    const project = makeProject({
      ".stylelintrc.json": config(rules),
      "a.css": EMPTY_BLOCK,
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(1);
  });

  test.failing("quiet: true drops warning-severity problems", () => {
    const project = makeProject({
      ".stylelintrc.json": config(rules, { quiet: true }),
      "a.css": EMPTY_BLOCK,
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test("quiet: true keeps error-severity problems", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { quiet: true }),
      "a.css": EMPTY_BLOCK,
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(1);
  });
});

describe("[tier 1] fix config key", () => {
  const rules = { "color-hex-case": "lower" };

  test("files are left alone when the key is absent", () => {
    const project = makeProject({
      ".stylelintrc.json": config(rules),
      "a.css": UPPER_HEX,
    });

    runGale(["a.css"], { cwd: project.dir });
    expect(project.read("a.css")).toBe(UPPER_HEX);
  });

  test.failing("fix: true rewrites the file in place", () => {
    const project = makeProject({
      ".stylelintrc.json": config(rules, { fix: true }),
      "a.css": UPPER_HEX,
    });

    const result = runGale(["a.css"], { cwd: project.dir });
    expect(project.read("a.css")).toBe("a { color: #fff; }\n");
    expect(result.exitCode).toBe(0);
  });

  test.failing('fix: "lax" is accepted like --fix=lax', () => {
    const project = makeProject({
      ".stylelintrc.json": config(rules, { fix: "lax" }),
      "a.css": UPPER_HEX,
    });

    runGale(["a.css"], { cwd: project.dir });
    expect(project.read("a.css")).toBe("a { color: #fff; }\n");
  });
});

describe("[tier 1] cache and cacheLocation config keys", () => {
  test("no cache file is written when the key is absent", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": "a { color: red; }\n",
    });

    runGale(["a.css"], { cwd: project.dir });
    expect(project.exists(".gale_cache")).toBe(false);
  });

  test.failing("cache: true writes the default cache file", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { cache: true }),
      "a.css": "a { color: red; }\n",
    });

    runGale(["a.css"], { cwd: project.dir });
    expect(project.exists(".gale_cache")).toBe(true);
  });

  test.failing("cacheLocation moves the cache file", () => {
    const project = makeProject({
      ".stylelintrc.json": config(
        { "block-no-empty": true },
        { cache: true, cacheLocation: "tmp/lint.cache" },
      ),
      "a.css": "a { color: red; }\n",
    });

    runGale(["a.css"], { cwd: project.dir });
    expect(project.exists("tmp/lint.cache")).toBe(true);
    expect(project.exists(".gale_cache")).toBe(false);
  });
});
