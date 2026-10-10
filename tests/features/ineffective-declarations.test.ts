/**
 * `gale/no-ineffective-declarations`: declarations that cannot do anything,
 * because another declaration in the same block switches them off.
 *
 * The rule only reports what the block itself proves. A `justify-content`
 * next to `display: block` does nothing; one with no `display` in its block
 * may be on a flex container that another rule made, so it is left alone.
 * Values that come from variables, blocks that include mixins, and
 * declarations that just set a property back to its initial value are left
 * alone too.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

const RULE = "gale/no-ineffective-declarations";

/** The warnings the rule reports for `code` in a file named `file`. */
function lint(code: string, file = "a.css") {
  const project = makeProject({
    ".stylelintrc.json": config({ [RULE]: true }),
    [file]: code,
  });
  return runGaleJson([file], { cwd: project.dir })
    .warnings()
    .map((w) => ({ line: w.line, column: w.column, text: w.text }));
}

/** The text the rule reports for `property` switched off by `cause`. */
function text(property: string, cause: string): string {
  return `Unexpected "${property}", which has no effect with "${cause}" (${RULE})`;
}

describe("reports", () => {
  test("flex and grid container properties on a block that is neither", () => {
    expect(lint("a {\n  display: block;\n  justify-content: center;\n  grid-template-columns: 1fr 1fr;\n}\n")).toEqual([
      { line: 3, column: 3, text: text("justify-content", "display: block") },
      { line: 4, column: 3, text: text("grid-template-columns", "display: block") },
    ]);
  });

  test("gap on an inline box that is not a multi-column container", () => {
    expect(lint("a { display: inline; gap: 1rem; }\n")).toEqual([
      { line: 1, column: 22, text: text("gap", "display: inline") },
    ]);
  });

  test("offsets on a statically positioned box", () => {
    expect(lint("a {\n  position: static;\n  top: 0;\n  inset-inline-start: 1rem;\n}\n")).toEqual([
      { line: 3, column: 3, text: text("top", "position: static") },
      { line: 4, column: 3, text: text("inset-inline-start", "position: static") },
    ]);
  });

  test("float on an absolutely or fixed positioned box", () => {
    expect(lint("a { position: absolute; float: left; }\nb { position: fixed; float: right; }\n")).toEqual([
      { line: 1, column: 25, text: text("float", "position: absolute") },
      { line: 2, column: 22, text: text("float", "position: fixed") },
    ]);
  });

  test("vertical-align on a block-level box", () => {
    expect(lint("a { display: flex; vertical-align: middle; }\n")).toEqual([
      { line: 1, column: 20, text: text("vertical-align", "display: flex") },
    ]);
  });

  test("table-layout on a box that is not a table", () => {
    expect(lint("a { display: grid; table-layout: fixed; }\n")).toEqual([
      { line: 1, column: 20, text: text("table-layout", "display: grid") },
    ]);
  });

  test("the same problems in SCSS", () => {
    expect(lint("a {\n  display: block;\n  justify-content: center;\n}\n", "a.scss")).toEqual([
      { line: 3, column: 3, text: text("justify-content", "display: block") },
    ]);
  });
});

describe("leaves alone", () => {
  const cases: Record<string, string> = {
    "a flex container": "a { display: flex; justify-content: center; gap: 1rem; }\n",
    "an inline-flex or two-keyword grid container": "a { display: inline-flex; align-items: center; }\nb { display: block grid; gap: 1rem; }\n",
    "a block with no display of its own": "a { justify-content: center; }\n",
    "a display that comes from a variable": "a { display: var(--display); justify-content: center; }\n",
    "the display that wins, not the first one written": "a { display: block; display: flex; justify-content: center; }\n",
    "an earlier !important display": "a { display: flex !important; display: block; justify-content: center; }\n",
    "display: none, which a block often toggles": "a { display: none; justify-content: center; }\n",
    "gap in a multi-column container": "a { display: block; column-count: 3; gap: 1rem; }\n",
    "a property set back to its initial value": "a { position: static; top: auto; float: none; }\n",
    "a CSS-wide keyword": "a { display: block; justify-content: inherit; }\n",
    "a nested rule, which is a block of its own": "a {\n  display: block;\n  &:hover { justify-content: center; }\n}\n",
    "relative positioning": "a { position: relative; top: 1px; float: left; }\n",
  };

  for (const [name, code] of Object.entries(cases)) {
    test(name, () => {
      expect(lint(code)).toEqual([]);
    });
  }

  test("a block that includes a mixin, which may change display", () => {
    expect(lint("a {\n  @include flex-row;\n  display: block;\n  justify-content: center;\n}\n", "a.scss")).toEqual([]);
  });

  test("a display set from a Sass variable", () => {
    expect(lint("a {\n  display: $display;\n  justify-content: center;\n}\n", "a.scss")).toEqual([]);
  });
});

describe("gale:strict", () => {
  test("turns the rule on", () => {
    const project = makeProject({
      ".stylelintrc.json": JSON.stringify({ extends: "gale:strict" }),
      "a.css": "a { display: block; justify-content: center; }\n",
    });

    const rules = runGaleJson(["a.css"], { cwd: project.dir })
      .warnings()
      .map((w) => w.rule);

    expect(rules).toContain(RULE);
  });
});
