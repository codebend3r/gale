/**
 * Rule options are checked once, when the config is read.
 *
 * Stylelint validates every rule's options before it lints anything: a value
 * the rule does not accept, or a secondary option it does not know, is an
 * invalid option on every result and the rule does not run.  It does not
 * fall back to the rule's default.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

/** The invalid-option texts and warning rules for `css` under `rules`. */
function check(rules: Record<string, unknown>, css: string) {
  const project = makeProject({ ".stylelintrc.json": config(rules), "a.css": css });
  const result = runGaleJson(["a.css"], { cwd: project.dir });
  const [source] = result.json();
  return {
    exitCode: result.exitCode,
    invalid: source.invalidOptionWarnings.map((w) => (w as { text: string }).text),
    rules: source.warnings.map((w) => w.rule),
  };
}

describe("an invalid primary option", () => {
  test.failing("is reported and the rule does not run", () => {
    const result = check({ "color-hex-length": "sideways" }, "a { color: #ffffff; }\n");
    expect(result.invalid).toEqual(['Invalid option value "sideways" for rule "color-hex-length"']);
    expect(result.rules).toEqual([]);
    expect(result.exitCode).toBe(2);
  });

  test.failing("is reported for a file with nothing in it", () => {
    expect(check({ "color-hex-length": "sideways" }, "").invalid).toEqual([
      'Invalid option value "sideways" for rule "color-hex-length"',
    ]);
  });

  test.failing("is reported when a list rule is given a number", () => {
    expect(check({ "property-allowed-list": 5 }, "a { color: red; }\n").invalid).toEqual([
      'Invalid option value "5" for rule "property-allowed-list"',
    ]);
  });
});

describe("an unknown secondary option", () => {
  test.failing("is reported by name", () => {
    expect(
      check({ "color-named": ["never", { ignoreEverything: true }] }, "a { color: red; }\n").invalid,
    ).toEqual(['Invalid option name "ignoreEverything" for rule "color-named"']);
  });
});

describe("valid options", () => {
  test("report nothing and the rule runs", () => {
    const result = check(
      { "color-hex-length": "short", "color-named": ["never", { ignoreProperties: ["fill"] }] },
      "a { color: #ffffff; }\n",
    );
    expect(result.invalid).toEqual([]);
    expect(result.rules).toEqual(["color-hex-length"]);
  });

  test("the secondary options every rule accepts are not unknown", () => {
    const result = check(
      { "color-hex-length": ["short", { severity: "warning", message: "Short hex", url: "https://x.test", disableFix: true, reportDisables: true }] },
      "a { color: #ffffff; }\n",
    );
    expect(result.invalid).toEqual([]);
  });
});
