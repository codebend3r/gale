/**
 * Agent guardrail mode: the `gale:strict` preset and the `agent` formatter.
 *
 * `gale:strict` is `gale:recommended` plus the rules teams use to keep
 * AI-written CSS in line: colours and spacing from variables, no
 * `!important`, no ID selectors, shallow nesting. Its own rules are errors,
 * so a violation fails the run and lands back in the agent's loop.
 *
 * The `agent` formatter prints one compiler-style line per problem, marks
 * the ones `gale --fix` can fix, ends with a one-line summary, and prints
 * nothing at all for a clean run.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

/** `.stylelintrc.json` text that extends `gale:strict`, plus `rules`. */
function strict(rules: Record<string, unknown> = {}): string {
  return JSON.stringify({ extends: "gale:strict", rules });
}

describe("gale:strict", () => {
  test.failing("turns the guardrail rules on as errors, on top of gale:recommended", () => {
    const project = makeProject({
      ".stylelintrc.json": strict(),
      "a.css": "a {}\n",
    });

    const result = runGale(["--print-config", "a.css"], { cwd: project.dir });
    const rules = JSON.parse(result.stdout).rules as Record<string, unknown>;

    for (const name of [
      "declaration-no-important",
      "selector-max-id",
      "max-nesting-depth",
      "color-named",
      "plugin/enforce-variable-for-property",
    ]) {
      const setting = rules[name];
      const severity = Array.isArray(setting) ? setting[0] : setting;
      expect(severity).toBe("error");
    }
    expect(rules).toHaveProperty("block-no-empty");
  });

  test.failing("rejects raw colours and spacing, and accepts variables", () => {
    const project = makeProject({
      ".stylelintrc.json": strict(),
      "a.css": [
        ".card {",
        "  color: #333;",
        "  background-color: var(--surface);",
        "  border-color: currentColor;",
        "  margin: 16px;",
        "  padding: 0 var(--space-2);",
        "}",
        "",
      ].join("\n"),
    });

    const result = runGaleJson(["a.css"], { cwd: project.dir });

    expect(result.exitCode).toBe(2);
    expect(result.warnings().map((w) => [w.line, w.rule, w.severity])).toEqual([
      [2, "plugin/enforce-variable-for-property", "error"],
      [5, "plugin/enforce-variable-for-property", "error"],
    ]);
  });

  test.failing("accepts Sass variables, module members and Less variables", () => {
    const project = makeProject({
      ".stylelintrc.json": strict(),
      "a.scss": [
        '@use "tokens";',
        ".card {",
        "  color: $text;",
        "  background-color: tokens.$surface;",
        "  margin: 0 auto;",
        "  padding: math.div($space, 2);",
        "  border-color: #000;",
        "}",
        "",
      ].join("\n"),
      "b.less": ".card {\n  color: @text;\n  margin: @space;\n}\n",
    });

    const result = runGaleJson(["a.scss", "b.less"], { cwd: project.dir });

    // Only the raw colour on line 7 of a.scss is reported.
    expect(result.warnings().map((w) => [w.line, w.rule])).toEqual([
      [7, "plugin/enforce-variable-for-property"],
    ]);
  });

  test.failing("rejects !important, ID selectors and nesting deeper than three levels", () => {
    const project = makeProject({
      ".stylelintrc.json": strict(),
      "a.css": [
        "#app .title { color: var(--text) !important; }",
        ".a { .b { .c { .d { .e { color: var(--text); } } } } }",
        "",
      ].join("\n"),
    });

    const rules = new Set(runGaleJson(["a.css"], { cwd: project.dir }).warnings().map((w) => w.rule));

    expect(rules).toContain("declaration-no-important");
    expect(rules).toContain("selector-max-id");
    expect(rules).toContain("max-nesting-depth");
  });

  test.failing("lets a project turn one of its rules back off", () => {
    const project = makeProject({
      ".stylelintrc.json": strict({ "declaration-no-important": null }),
      "a.css": "#app { color: var(--text) !important; }\n",
    });

    const result = runGaleJson(["a.css"], { cwd: project.dir });

    expect(result.warnings().map((w) => w.rule)).toEqual(["selector-max-id"]);
  });
});

describe("agent formatter", () => {
  const files = {
    ".stylelintrc.json": config({
      "color-hex-length": "short",
      "block-no-empty": [true, { severity: "warning" }],
    }),
    "a.css": "a { color: #ffffff; }\nb {}\n",
  };

  test.failing("prints one line per problem, marks the fixable ones, and sums up", () => {
    const project = makeProject(files);

    const json = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    const result = runGale(["--formatter", "agent", "a.css"], { cwd: project.dir });

    // Built from the JSON report, so the test pins the layout, not the
    // wording of the two messages.
    const message = (rule: string) => {
      const warning = json.find((w) => w.rule === rule)!;
      return `${warning.line}:${warning.column}: ${warning.severity}: ${warning.text.replace(` (${rule})`, "")}`;
    };
    expect(result.exitCode).toBe(2);
    expect(result.stdout).toBe(
      [
        `a.css:${message("color-hex-length")} [color-hex-length, fixable]`,
        `a.css:${message("block-no-empty")} [block-no-empty]`,
        "2 problems (1 error, 1 warning), 1 fixable with `gale --fix`",
        "",
      ].join("\n"),
    );
  });

  test.failing("is chosen by the config's formatter key too", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { formatter: "agent" }),
      "a.css": "a {}\n",
    });

    const result = runGale(["a.css"], { cwd: project.dir });

    expect(result.stdout).toMatch(/^a\.css:1:3: error: .+ \[block-no-empty\]\n1 problem \(1 error, 0 warnings\)\n$/);
  });

  test.failing("prints nothing when there are no problems", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": "a { color: red; }\n",
    });

    const result = runGale(["--formatter", "agent", "a.css"], { cwd: project.dir });

    expect(result.stdout).toBe("");
    expect(result.exitCode).toBe(0);
  });

  test.failing("after --fix, lists only what is left and leaves out the fixable clause", () => {
    const project = makeProject(files);

    const result = runGale(["--fix", "--formatter", "agent", "a.css"], { cwd: project.dir });

    expect(result.stdout).toMatch(/^a\.css:2:3: warning: .+ \[block-no-empty\]\n1 problem \(0 errors, 1 warning\)\n$/);
    expect(project.read("a.css")).toBe("a { color: #fff; }\nb {}\n");
  });
});
