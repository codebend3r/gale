/**
 * Colour in the human-readable formatters.
 *
 * Stylelint uses picocolors, whose decision is:
 *   colour = !(NO_COLOR || --no-color)
 *         && (FORCE_COLOR || --color || stdout is a TTY || CI)
 *
 * Every test here pipes stdout, so the TTY branch is always false and the
 * remaining inputs are controlled one at a time.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { EMPTY_BLOCK, cleanupProjects, config, hasAnsi, makeProject, runGale } from "./helpers";

afterAll(cleanupProjects);

function project() {
  return makeProject({
    ".stylelintrc.json": config({ "block-no-empty": true }),
    "a.css": EMPTY_BLOCK,
  });
}

describe("colour detection", () => {
  test("piped output has no colour by default", () => {
    const result = runGale(["a.css"], { cwd: project().dir });

    expect(result.stdout).toContain("block-no-empty");
    expect(hasAnsi(result.stdout)).toBe(false);
  });

  test("--color forces colour on", () => {
    const result = runGale(["--color", "a.css"], { cwd: project().dir });

    expect(hasAnsi(result.stdout)).toBe(true);
  });

  test("--no-color forces colour off", () => {
    const result = runGale(["--no-color", "a.css"], { cwd: project().dir });

    expect(result.stdout).toContain("block-no-empty");
    expect(hasAnsi(result.stdout)).toBe(false);
  });

  test("FORCE_COLOR turns colour on", () => {
    const result = runGale(["a.css"], { cwd: project().dir, env: { FORCE_COLOR: "1" } });

    expect(hasAnsi(result.stdout)).toBe(true);
  });

  test("CI turns colour on", () => {
    const result = runGale(["a.css"], { cwd: project().dir, env: { CI: "true" } });

    expect(hasAnsi(result.stdout)).toBe(true);
  });

  test("NO_COLOR turns colour off", () => {
    const result = runGale(["a.css"], { cwd: project().dir, env: { NO_COLOR: "1" } });

    expect(result.stdout).toContain("block-no-empty");
    expect(hasAnsi(result.stdout)).toBe(false);
  });

  test("NO_COLOR beats FORCE_COLOR", () => {
    const result = runGale(["a.css"], {
      cwd: project().dir,
      env: { NO_COLOR: "1", FORCE_COLOR: "1" },
    });

    expect(result.stdout).toContain("block-no-empty");
    expect(hasAnsi(result.stdout)).toBe(false);
  });

  test("--no-color beats FORCE_COLOR", () => {
    const result = runGale(["--no-color", "a.css"], {
      cwd: project().dir,
      env: { FORCE_COLOR: "1" },
    });

    expect(result.stdout).toContain("block-no-empty");
    expect(hasAnsi(result.stdout)).toBe(false);
  });

  test("the verbose formatter follows the same rule", () => {
    const dir = project().dir;

    expect(hasAnsi(runGale(["--formatter", "verbose", "a.css"], { cwd: dir }).stdout)).toBe(false);
    expect(
      hasAnsi(runGale(["--formatter", "verbose", "--color", "a.css"], { cwd: dir }).stdout),
    ).toBe(true);
  });

  test("the json formatter ignores FORCE_COLOR", () => {
    const result = runGale(["--formatter", "json", "a.css"], {
      cwd: project().dir,
      env: { FORCE_COLOR: "1" },
    });

    expect(hasAnsi(result.stdout)).toBe(false);
    expect(() => JSON.parse(result.stdout)).not.toThrow();
  });

  test("the json formatter ignores --color", () => {
    const result = runGale(["--formatter", "json", "--color", "a.css"], { cwd: project().dir });

    expect(hasAnsi(result.stdout)).toBe(false);
    expect(() => JSON.parse(result.stdout)).not.toThrow();
  });
});
