/**
 * Rule names gale does not run are never dropped silently.
 *
 * A name that is no rule at all gets Stylelint's own report: an error at
 * 1:1 of every file, `Unknown rule <name>.`, with a "Did you mean" list of
 * close core rules. A real Stylelint or plugin rule that gale has not
 * implemented yet is skipped with one stderr warning per run, so a
 * migration is not failed by rules gale has yet to port; so is a rule an
 * older Stylelint had, which configs written for it still carry.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

describe("a name that is no rule", () => {
  test("is reported on every file the way Stylelint reports it", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-emty": true }),
      "a.css": "a { color: red; }\n",
      "b.css": "b { color: red; }\n",
    });

    const result = runGaleJson(["a.css", "b.css"], { cwd: project.dir });

    expect(result.exitCode).toBe(2);
    for (const source of result.json()) {
      expect(source.errored).toBe(true);
      expect(source.warnings).toEqual([
        {
          line: 1,
          column: 1,
          endLine: 1,
          endColumn: 2,
          rule: "block-no-emty",
          severity: "error",
          text: "Unknown rule block-no-emty. Did you mean block-no-empty?",
        },
      ]);
    }
  });

  test("leaves out the suggestion when nothing is close", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "zzzzzzzzzzzzzzzzzzzzzzzzzz": true }),
      "a.css": "a {}\n",
    });

    const [warning] = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect(warning.text).toBe("Unknown rule zzzzzzzzzzzzzzzzzzzzzzzzzz.");
  });

  test("does not stop the configured rules from running", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-emty": true, "block-no-empty": true }),
      "a.css": "a {}\n",
    });

    const rules = runGaleJson(["a.css"], { cwd: project.dir })
      .warnings()
      .map((w) => w.rule);
    expect(rules.sort()).toEqual(["block-no-empty", "block-no-emty"]);
  });
});

describe("a rule gale has not implemented yet", () => {
  test("is skipped with one warning listing every such rule", () => {
    const project = makeProject({
      ".stylelintrc.json": config({
        "no-unknown-custom-properties": true,
        "acme/no-foo": true,
        "block-no-empty": true,
      }),
      "a.css": "a { color: red; }\n",
      "b.css": "b { color: red; }\n",
    });

    const result = runGale(["a.css", "b.css"], { cwd: project.dir });

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toBe("");
    const warnings = result.stderr.split("\n").filter((l) => l.startsWith("warning:"));
    expect(warnings).toEqual([
      "warning: gale does not support these rules yet, so they were skipped: " +
        "acme/no-foo, no-unknown-custom-properties",
    ]);
  });

  test("is not reported as an unknown rule", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "no-unknown-custom-properties": true }),
      "a.css": "a { color: var(--nope); }\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([]);
  });
});

describe("a rule an older Stylelint had", () => {
  test("is skipped with its own warning instead of failing the run", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ linebreaks: "unix", "function-whitelist": ["calc"] }),
      "a.css": "a { color: red; }\n",
    });

    const result = runGale(["a.css"], { cwd: project.dir });

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toBe("");
    expect(result.stderr).toContain(
      "warning: these rules were removed from Stylelint, so they were skipped: " +
        "function-whitelist, linebreaks",
    );
  });
});
