/**
 * stylelint-scss rules that gale registers and enables in its SCSS presets.
 *
 * A rule gale lists as implemented has to report what stylelint-scss
 * reports; these cases come from each rule's own README and test suite.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

/** The warnings for `scss` linted under `rules`, as `line:column text`. */
function lint(rules: Record<string, unknown>, scss: string): string[] {
  const project = makeProject({ ".stylelintrc.json": config(rules), "a.scss": scss });
  return runGaleJson(["a.scss"], { cwd: project.dir })
    .warnings()
    .map((w) => `${w.line}:${w.column} ${w.text}`);
}

describe("scss/operator-no-newline-before", () => {
  const rules = { "scss/operator-no-newline-before": true };

  test.failing("flags an operator that starts a line", () => {
    expect(lint(rules, "a {\n  width: 10px\n  + 1;\n}\n")).toEqual([
      '3:3 Unexpected newline before "+" (scss/operator-no-newline-before)',
    ]);
  });

  test("allows a newline after the operator", () => {
    expect(lint(rules, "a {\n  width: 10px +\n    1;\n}\n")).toEqual([]);
  });

  test("allows a sign that is not an operator", () => {
    expect(lint(rules, "a {\n  width: 10px\n    -1;\n}\n")).toEqual([]);
  });
});

describe("scss/declaration-nested-properties-no-divided-groups", () => {
  const rules = { "scss/declaration-nested-properties-no-divided-groups": true };

  test.failing("flags each group of a namespace split in two", () => {
    const scss = [
      "a {",
      "  background: url(img.png) center {",
      "    size: auto;",
      "  }",
      "  background: {",
      "    repeat: no-repeat;",
      "  }",
      "}",
      "",
    ].join("\n");
    const message =
      'Expected all nested properties of "background" namespace to be in one nested group ' +
      "(scss/declaration-nested-properties-no-divided-groups)";
    expect(lint(rules, scss)).toEqual([`2:3 ${message}`, `5:3 ${message}`]);
  });

  test("allows one group next to a plain longhand", () => {
    const scss = "a {\n  background: url(img.png) center {\n    size: auto;\n  }\n  background-repeat: no-repeat;\n}\n";
    expect(lint(rules, scss)).toEqual([]);
  });
});

describe("scss/dollar-variable-no-missing-interpolation", () => {
  const rules = { "scss/dollar-variable-no-missing-interpolation": true };
  const scss = '$var: "my-anim";\n.a {\n  animation-name: $var;\n}\n';

  test.failing("flags a string variable used as a custom identifier", () => {
    expect(lint(rules, scss)).toEqual([
      "3:19 Expected variable $var to be interpolated when using it with animation-name " +
        "(scss/dollar-variable-no-missing-interpolation)",
    ]);
  });

  test.failing("--fix interpolates the variable", () => {
    const project = makeProject({ ".stylelintrc.json": config(rules), "a.scss": scss });
    runGale(["--fix", "a.scss"], { cwd: project.dir });
    expect(project.read("a.scss")).toBe('$var: "my-anim";\n.a {\n  animation-name: #{$var};\n}\n');
  });

  test("allows an interpolated variable", () => {
    expect(lint(rules, '$var: "my-anim";\n.a {\n  animation-name: #{$var};\n}\n')).toEqual([]);
  });
});
