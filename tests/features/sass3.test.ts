/**
 * Dart Sass 3 readiness.
 *
 * Dart Sass deprecated `@import` and the global built-in functions in 1.80.0
 * and plans to remove them in 3.0.0. This file pins:
 *
 * - the stylelint-scss rules for the module system and the colour functions
 *   that gale has not had, with stylelint-scss's own messages;
 * - `gale/scss-no-import`, which reports a Sass `@import` but leaves the
 *   imports Sass passes through as plain CSS alone;
 * - an autofix for `scss/no-global-function-names` that rewrites a global
 *   call to its module function and adds the `@use` it needs;
 * - the `gale:sass3` preset, which turns all of that on.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

/** The warnings `rule` reports for `code` in an `a.scss` file. */
function lint(rule: string, code: string) {
  const project = makeProject({
    ".stylelintrc.json": config({ [rule]: true }),
    "a.scss": code,
  });
  return runGaleJson(["a.scss"], { cwd: project.dir })
    .warnings()
    .map((w) => ({ line: w.line, rule: w.rule, text: w.text }));
}

describe("stylelint-scss module rules", () => {
  test.failing("scss/at-use-no-unnamespaced", () => {
    expect(lint("scss/at-use-no-unnamespaced", '@use "foo" as *;\n@use "bar" as b;\n')).toEqual([
      {
        line: 1,
        rule: "scss/at-use-no-unnamespaced",
        text: "Unexpected @use without namespace (scss/at-use-no-unnamespaced)",
      },
    ]);
  });

  test.failing("scss/at-use-no-redundant-alias", () => {
    expect(
      lint("scss/at-use-no-redundant-alias", '@use "sass:math" as math;\n@use "src/corners" as c;\n'),
    ).toEqual([
      {
        line: 1,
        rule: "scss/at-use-no-redundant-alias",
        text: "Unexpected redundant namespace. (scss/at-use-no-redundant-alias)",
      },
    ]);
  });

  test.failing("scss/no-duplicate-load-rules", () => {
    expect(lint("scss/no-duplicate-load-rules", '@use "foo";\n@use "foo" as f;\n@use "bar";\n')).toEqual([
      {
        line: 2,
        rule: "scss/no-duplicate-load-rules",
        text: "Unexpected duplicate load rule foo (scss/no-duplicate-load-rules)",
      },
    ]);
  });

  test.failing("scss/dollar-variable-no-namespaced-assignment", () => {
    expect(
      lint("scss/dollar-variable-no-namespaced-assignment", "imported.$foo: 1;\na { b: imported.$foo; }\n"),
    ).toEqual([
      {
        line: 1,
        rule: "scss/dollar-variable-no-namespaced-assignment",
        text: "Unexpected assignment to a namespaced $ variable (scss/dollar-variable-no-namespaced-assignment)",
      },
    ]);
  });
});

describe("stylelint-scss colour function rules", () => {
  test.failing("scss/function-color-channel", () => {
    expect(
      lint(
        "scss/function-color-channel",
        'p {\n  opacity: alpha(#abcdef80);\n  width: color.channel($c, "alpha");\n}\n',
      ),
    ).toEqual([
      {
        line: 2,
        rule: "scss/function-color-channel",
        text: "Expected the color.channel function to be used (scss/function-color-channel)",
      },
    ]);
  });

  test.failing("scss/function-color-relative", () => {
    expect(
      lint("scss/function-color-relative", "p {\n  color: darken(blue, .2);\n  background: scale-color(blue, $lightness: 10%);\n}\n"),
    ).toEqual([
      {
        line: 2,
        rule: "scss/function-color-relative",
        text: "Expected the scale-color function to be used (scss/function-color-relative)",
      },
    ]);
  });
});

describe("gale/scss-no-import", () => {
  test.failing("reports a Sass @import", () => {
    expect(lint("gale/scss-no-import", '@import "variables";\n@import "mixins", "functions";\n')).toEqual([
      {
        line: 1,
        rule: "gale/scss-no-import",
        text: "Expected @use or @forward instead of @import (gale/scss-no-import)",
      },
      {
        line: 2,
        rule: "gale/scss-no-import",
        text: "Expected @use or @forward instead of @import (gale/scss-no-import)",
      },
    ]);
  });

  test.failing("leaves the imports Sass keeps as plain CSS alone", () => {
    const code = [
      '@import "variables";',
      '@import "theme.css";',
      '@import url("fonts.css");',
      '@import "https://example.com/reset";',
      '@import "print" print;',
      '@import "layout" supports(display: grid);',
      "",
    ].join("\n");

    expect(lint("gale/scss-no-import", code).map((w) => w.line)).toEqual([1]);
  });

  test("never reports @import in a CSS file", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "gale/scss-no-import": true }),
      "a.css": '@import "variables";\n',
    });

    const result = runGaleJson(["a.css"], { cwd: project.dir });

    expect(result.warnings().filter((w) => w.rule === "gale/scss-no-import")).toEqual([]);
  });
});

describe("scss/no-global-function-names --fix", () => {
  /** `code` after `gale --fix` with only `scss/no-global-function-names` on. */
  function fix(code: string): string {
    const project = makeProject({
      ".stylelintrc.json": config({ "scss/no-global-function-names": true }),
      "a.scss": code,
    });
    runGale(["--fix", "a.scss"], { cwd: project.dir });
    return project.read("a.scss");
  }

  test.failing("renames the call and adds the @use it needs", () => {
    expect(fix("a {\n  b: map-get($m, a);\n}\n")).toBe('@use "sass:map";\n\na {\n  b: map.get($m, a);\n}\n');
  });

  test.failing("adds the @use after the file's other @use rules", () => {
    expect(fix('@use "config";\n\na {\n  b: str-length("x");\n}\n')).toBe(
      '@use "config";\n@use "sass:string";\n\na {\n  b: string.length("x");\n}\n',
    );
  });

  test.failing("uses the namespace the file already gave the module", () => {
    expect(fix('@use "sass:map" as m;\n\na {\n  b: map-get($m, a);\n}\n')).toBe(
      '@use "sass:map" as m;\n\na {\n  b: m.get($m, a);\n}\n',
    );
  });

  test("leaves calls whose arguments change, such as darken(), as they are", () => {
    const code = "a {\n  color: darken($c, 10%);\n}\n";
    expect(fix(code)).toBe(code);
  });

  test("leaves a module loaded without a namespace as it is", () => {
    const code = '@use "sass:map" as *;\n\na {\n  b: map-get($m, a);\n}\n';
    expect(fix(code)).toBe(code);
  });
});

describe("gale:sass3", () => {
  test.failing("turns on the Dart Sass 3 rules", () => {
    const project = makeProject({
      ".stylelintrc.json": JSON.stringify({ extends: "gale:sass3" }),
      "a.scss": "a {}\n",
    });

    const result = runGale(["--print-config", "a.scss"], { cwd: project.dir });
    const rules = Object.keys(JSON.parse(result.stdout).rules as Record<string, unknown>);

    expect(rules).toEqual(
      expect.arrayContaining([
        "gale/scss-no-import",
        "scss/no-global-function-names",
        "scss/function-color-channel",
        "scss/function-color-relative",
        "scss/no-duplicate-load-rules",
        "scss/dollar-variable-no-namespaced-assignment",
      ]),
    );
  });

  test.failing("reports what Dart Sass 3 removes", () => {
    const project = makeProject({
      ".stylelintrc.json": JSON.stringify({ extends: "gale:sass3" }),
      "a.scss": '@import "variables";\n\na {\n  b: map-get($m, a);\n}\n',
    });

    const rules = runGaleJson(["a.scss"], { cwd: project.dir })
      .warnings()
      .map((w) => w.rule);

    expect(rules).toEqual(["gale/scss-no-import", "scss/no-global-function-names"]);
  });
});
