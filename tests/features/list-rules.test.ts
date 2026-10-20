/**
 * The `*-allowed-list` and `*-disallowed-list` rules.
 *
 * Each takes a list of names (strings or `/regex/` literals) as its primary
 * option, optionally followed by a secondary options object.  These pin that
 * every list rule reads the user's list, in either shape, rather than a
 * built-in table.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

/** The warnings for `css` linted under `rules`, as `rule@line:column`. */
function lint(rules: Record<string, unknown>, css: string): string[] {
  const project = makeProject({ ".stylelintrc.json": config(rules), "a.css": css });
  return runGaleJson(["a.css"], { cwd: project.dir })
    .warnings()
    .map((w) => `${w.rule}@${w.line}:${w.column}`);
}

describe("disallowed lists read the configured list", () => {
  test.failing("property-disallowed-list flags a listed property", () => {
    expect(lint({ "property-disallowed-list": ["color"] }, "a { color: red; margin: 0; }\n")).toEqual([
      "property-disallowed-list@1:5",
    ]);
  });

  test.failing("property-disallowed-list matches a /regex/ entry", () => {
    expect(
      lint({ "property-disallowed-list": ["/^border/"] }, "a { border-top: 0; margin: 0; }\n"),
    ).toEqual(["property-disallowed-list@1:5"]);
  });

  test.failing("selector-disallowed-list flags a listed selector", () => {
    expect(lint({ "selector-disallowed-list": ["a > b"] }, "a > b { color: red; }\n")).toEqual([
      "selector-disallowed-list@1:1",
    ]);
  });

  test.failing("function-disallowed-list flags a listed function", () => {
    expect(
      lint({ "function-disallowed-list": ["rgba"] }, "a { color: rgba(0, 0, 0, 0); }\n"),
    ).toEqual(["function-disallowed-list@1:12"]);
  });
});

describe("allowed lists read the configured list", () => {
  test.failing("at-rule-allowed-list flags an at-rule missing from the list", () => {
    expect(
      lint({ "at-rule-allowed-list": ["media"] }, "@supports (display: grid) {}\n@media screen {}\n"),
    ).toEqual(["at-rule-allowed-list@1:1"]);
  });

  test("property-allowed-list flags a property missing from the list", () => {
    expect(
      lint({ "property-allowed-list": ["color"] }, "a { color: red; margin: 0; }\n"),
    ).toEqual(["property-allowed-list@1:17"]);
  });
});

describe("a list followed by secondary options", () => {
  test("property-allowed-list still allows the listed properties", () => {
    expect(
      lint(
        { "property-allowed-list": [["color", "margin"], { severity: "warning" }] },
        "a { color: red; margin: 0; padding: 0; }\n",
      ),
    ).toEqual(["property-allowed-list@1:28"]);
  });

  test.failing("property-disallowed-list still flags the listed properties", () => {
    expect(
      lint(
        { "property-disallowed-list": [["color"], { severity: "warning" }] },
        "a { color: red; margin: 0; }\n",
      ),
    ).toEqual(["property-disallowed-list@1:5"]);
  });

  test("unit-allowed-list still allows the listed units", () => {
    expect(
      lint({ "unit-allowed-list": [["px"], { severity: "warning" }] }, "a { width: 1px; height: 1em; }\n"),
    ).toEqual(["unit-allowed-list@1:25"]);
  });

  test("the secondary severity applies to the list rule's warnings", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "unit-allowed-list": [["px"], { severity: "warning" }] }),
      "a.css": "a { height: 1em; }\n",
    });
    const [warning] = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect(warning.severity).toBe("warning");
  });
});
