/**
 * Which lines `stylelint-disable` comments turn rules off on.
 *
 * Stylelint's ranges are whole lines: a `stylelint-disable` covers its own
 * line (code before the comment included) through the line of the
 * `stylelint-enable` that ends it, `stylelint-disable-line` the comment's
 * first line, and `stylelint-disable-next-line` the line after its last.
 * The expected lines below are the ones Stylelint 17 reports for the same
 * input.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

const RULES = {
  "block-no-empty": true,
  "comment-whitespace-inside": "always",
};

/** The `line rule` of every warning gale reports for `file` in `files`. */
function lines(files: Record<string, string>, file: string, rules: object = RULES): string[] {
  const project = makeProject({ ".stylelintrc.json": config(rules as Record<string, unknown>), ...files });
  return runGaleJson([file], { cwd: project.dir })
    .warnings()
    .map((w) => `${w.line} ${w.rule}`);
}

describe("disable ranges are whole lines", () => {
  test("a disable comment is itself disabled", () => {
    // The command breaks the rule it turns off; Stylelint does not report it.
    expect(lines({ "a.css": "/*stylelint-disable comment-whitespace-inside*/\n/*x*/\n" }, "a.css")).toEqual(
      [],
    );
  });

  test("code before a disable on its line is disabled", () => {
    expect(lines({ "a.css": "a {} /* stylelint-disable block-no-empty */\nb {}\n" }, "a.css")).toEqual([]);
  });

  test("the enable comment's line is still disabled", () => {
    const source = [
      "/* stylelint-disable block-no-empty */",
      "a {}",
      "/* stylelint-enable block-no-empty */ b {}",
      "c {}",
      "",
    ].join("\n");
    expect(lines({ "a.css": source }, "a.css")).toEqual(["4 block-no-empty"]);
  });

  test("disable-next-line reaches the line after a multi-line comment", () => {
    const source = "/* stylelint-disable-next-line\n   block-no-empty */\na {}\nb {}\n";
    expect(lines({ "a.css": source }, "a.css")).toEqual(["4 block-no-empty"]);
  });

  test("disable-line covers code before and after the comment", () => {
    const source = "a {} /* stylelint-disable-line */ b {}\nc {}\n";
    expect(lines({ "a.css": source }, "a.css")).toEqual(["2 block-no-empty"]);
  });

  test("a command inside a selector counts, one inside a string does not", () => {
    const source = [
      "a, /* stylelint-disable-line block-no-empty */ b {}",
      'c { content: "/* stylelint-disable */"; }',
      "d {}",
      "",
    ].join("\n");
    expect(lines({ "a.css": source }, "a.css")).toEqual(["3 block-no-empty"]);
  });
});

describe("// comments", () => {
  test("are commands in SCSS and Less", () => {
    const source = "// stylelint-disable-next-line block-no-empty\na {}\nb {}\n";
    expect(lines({ "a.scss": source }, "a.scss")).toEqual(["3 block-no-empty"]);
    expect(lines({ "a.less": source }, "a.less")).toEqual(["3 block-no-empty"]);
  });

  test("are not comments in CSS", () => {
    const source = "a { color: #ab; } // stylelint-disable-line color-no-invalid-hex\nb { color: red; }\n";
    expect(lines({ "a.css": source }, "a.css", { "color-no-invalid-hex": true })).toEqual([
      "1 color-no-invalid-hex",
    ]);
  });

  test("a description on the next lines carries the command down", () => {
    // Stylelint reads the run of `//` comments as one, so the next line is
    // the one after the description.
    const source = [
      "// stylelint-disable-next-line block-no-empty",
      "// -- generated markup",
      "a {}",
      "b {}",
      "",
    ].join("\n");
    expect(lines({ "a.scss": source }, "a.scss")).toEqual(["4 block-no-empty"]);
  });

  test("inside an SCSS value they count, inside a Less value they do not", () => {
    const source = "a {\n  box-shadow: 0 0 #ab, // stylelint-disable-line color-no-invalid-hex\n    0 0 red;\n}\n";
    const rules = { "color-no-invalid-hex": true };
    expect(lines({ "a.scss": source }, "a.scss", rules)).toEqual([]);
    expect(lines({ "a.less": source }, "a.less", rules)).toEqual(["2 color-no-invalid-hex"]);
  });
});

describe("embedded style blocks", () => {
  test("ranges run across blocks on host-file lines", () => {
    const vue = [
      "<template><p>x</p></template>",
      "<style>",
      "/* stylelint-disable block-no-empty */",
      "a {}",
      "</style>",
      "<style>",
      "b {}",
      "/* stylelint-enable block-no-empty */ c {}",
      "d {}",
      "</style>",
      "",
    ].join("\n");
    expect(lines({ "App.vue": vue }, "App.vue")).toEqual(["9 block-no-empty"]);
  });
});
