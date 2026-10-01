/**
 * Gale's built-in presets.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, makeProject, runGale } from "./helpers";

afterAll(cleanupProjects);

describe("gale:all", () => {
  test("enables every rule, including the selector validity rules", () => {
    const project = makeProject({
      ".stylelintrc.json": JSON.stringify({ extends: "gale:all" }),
      "a.css": "a {}\n",
    });

    const result = runGale(["--print-config", "a.css"], { cwd: project.dir });
    const rules = Object.keys(JSON.parse(result.stdout).rules as Record<string, unknown>);

    expect(rules).toContain("selector-no-invalid");
    expect(rules).toContain("selector-no-unmatchable");
    expect(rules.length).toBeGreaterThan(250);
  });
});
