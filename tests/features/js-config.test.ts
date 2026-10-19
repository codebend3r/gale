/**
 * JavaScript configs: `stylelint.config.js`, `.mjs` and `.cjs`.
 *
 * Stylelint runs these as modules, so any JavaScript is fair game.  Gale runs
 * them under Node when Node is on `PATH`, and only otherwise falls back to
 * reading the file as a static object literal, saying what it had to skip.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { EMPTY_BLOCK, cleanupProjects, makeProject, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

/** The rules that fire on an empty block under the JS config `source`. */
function rulesFor(source: string, file = "stylelint.config.js", env: Record<string, string> = {}) {
  const project = makeProject({ [file]: source, "a.css": EMPTY_BLOCK });
  const result = runGaleJson(["a.css"], { cwd: project.dir, env });
  return result.warnings().map((w) => w.rule);
}

describe("a JS config is run as JavaScript", () => {
  test("a plain exported object", () => {
    expect(rulesFor('module.exports = { rules: { "block-no-empty": true } };\n')).toEqual([
      "block-no-empty",
    ]);
  });

  test("an ES module default export", () => {
    expect(
      rulesFor('export default { rules: { "block-no-empty": true } };\n', "stylelint.config.mjs"),
    ).toEqual(["block-no-empty"]);
  });

  test("a regular expression option", () => {
    const project = makeProject({
      "stylelint.config.js": 'module.exports = { rules: { "selector-class-pattern": /^[a-z]+$/ } };\n',
      "a.css": ".Ab {}\n",
    });
    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings().map((w) => w.rule)).toEqual([
      "selector-class-pattern",
    ]);
  });

  test.failing("object spread", () => {
    expect(
      rulesFor(
        'const base = { "block-no-empty": true };\nmodule.exports = { rules: { ...base } };\n',
      ),
    ).toEqual(["block-no-empty"]);
  });

  test.failing("rules built with map and Object.fromEntries", () => {
    expect(
      rulesFor(
        'module.exports = {\n  rules: Object.fromEntries(["block-no-empty"].map((r) => [r, true])),\n};\n',
      ),
    ).toEqual(["block-no-empty"]);
  });

  test.failing("a value read from process.env", () => {
    expect(
      rulesFor(
        'module.exports = { rules: { "block-no-empty": process.env.GALE_UNSET ? null : true } };\n',
      ),
    ).toEqual(["block-no-empty"]);
  });

  test.failing("a config assembled by statements", () => {
    expect(
      rulesFor(
        'const rules = {};\nrules["block-no-empty"] = true;\nmodule.exports = { rules };\n',
        "stylelint.config.cjs",
      ),
    ).toEqual(["block-no-empty"]);
  });

  test.failing("a constant's name inside a string is left alone", () => {
    const project = makeProject({
      "stylelint.config.js":
        'const OFF = null;\nmodule.exports = {\n  rules: { "block-no-empty": [true, { message: "OFF is not allowed" }] },\n};\n',
      "a.css": EMPTY_BLOCK,
    });
    const [warning] = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect(warning.text).toBe("OFF is not allowed (block-no-empty)");
  });

  test.failing("non-ASCII text after a constant does not crash the loader", () => {
    expect(
      rulesFor(
        'const OFF = null;\nmodule.exports = {\n  rules: { "block-no-empty": [true, { message: "Empty — remove it" }] },\n};\n',
      ),
    ).toEqual(["block-no-empty"]);
  });

  test.failing("a config that throws is a configuration error", () => {
    const project = makeProject({
      "stylelint.config.js": 'throw new Error("boom");\n',
      "a.css": EMPTY_BLOCK,
    });
    const result = runGale(["a.css"], { cwd: project.dir });
    expect(result.exitCode).toBe(78);
    expect(result.stderr).toContain("boom");
  });
});

describe("without Node on PATH", () => {
  // An empty PATH: gale is run by absolute path and needs nothing else.
  const noNode = { PATH: "/gale-no-node" };

  test("a plain exported object is still read", () => {
    expect(
      rulesFor('module.exports = { rules: { "block-no-empty": true } };\n', undefined, noNode),
    ).toEqual(["block-no-empty"]);
  });

  test.failing("says what the static reader could not evaluate", () => {
    const project = makeProject({
      "stylelint.config.js":
        'const base = { "block-no-empty": true };\nmodule.exports = { rules: { ...base } };\n',
      "a.css": EMPTY_BLOCK,
    });
    const result = runGale(["a.css"], { cwd: project.dir, env: noNode });
    expect(result.stderr).toContain("Node.js");
    expect(result.stderr).toContain("...base");
  });
});
