/**
 * Process exit codes, matching Stylelint v17's constants:
 *
 *   0  success
 *   1  fatal error (including "no files found")
 *   2  lint problems, or --max-warnings exceeded
 *   64 invalid usage
 *   78 invalid configuration
 */

import { afterAll, describe, expect, test } from "bun:test";

import { EMPTY_BLOCK, cleanupProjects, config, makeProject, runGale } from "./helpers";

afterAll(cleanupProjects);

describe("exit codes", () => {
  test("0 when the file is clean", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": "a { color: red; }\n",
    });

    expect(runGale(["a.css"], { cwd: project.dir }).exitCode).toBe(0);
  });

  test("0 when only warnings are found", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": [true, { severity: "warning" }] }),
      "a.css": EMPTY_BLOCK,
    });

    expect(runGale(["a.css"], { cwd: project.dir }).exitCode).toBe(0);
  });

  test("2 when an error-severity problem is found", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": EMPTY_BLOCK,
    });

    expect(runGale(["a.css"], { cwd: project.dir }).exitCode).toBe(2);
  });

  test("2 when --max-warnings is exceeded", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": [true, { severity: "warning" }] }),
      "a.css": EMPTY_BLOCK,
    });

    expect(runGale(["--max-warnings", "0", "a.css"], { cwd: project.dir }).exitCode).toBe(2);
  });

  test("64 for an unknown flag", () => {
    const project = makeProject({ "a.css": "a { color: red; }\n" });

    expect(runGale(["--definitely-not-a-flag", "a.css"], { cwd: project.dir }).exitCode).toBe(64);
  });

  test("64 for an unknown formatter", () => {
    const project = makeProject({ "a.css": "a { color: red; }\n" });

    expect(runGale(["--formatter", "nope", "a.css"], { cwd: project.dir }).exitCode).toBe(64);
  });

  test("78 when --config points at a missing file", () => {
    const project = makeProject({ "a.css": "a { color: red; }\n" });

    const result = runGale(["--config", "missing.json", "a.css"], { cwd: project.dir });
    expect(result.exitCode).toBe(78);
  });

  test("78 when the discovered config cannot be parsed", () => {
    const project = makeProject({
      ".stylelintrc.json": '{ "rules": { "block-no-empty": true ',
      "a.css": "a { color: red; }\n",
    });

    expect(runGale(["a.css"], { cwd: project.dir }).exitCode).toBe(78);
  });

  test("1 when no files match", () => {
    const project = makeProject({ ".stylelintrc.json": config({ "block-no-empty": true }) });

    expect(runGale(["missing/**/*.css"], { cwd: project.dir }).exitCode).toBe(1);
  });

  test("0 when no files match and --allow-empty-input is set", () => {
    const project = makeProject({ ".stylelintrc.json": config({ "block-no-empty": true }) });

    expect(
      runGale(["--allow-empty-input", "missing/**/*.css"], { cwd: project.dir }).exitCode,
    ).toBe(0);
  });
});
