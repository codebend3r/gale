/**
 * Regular expressions in rule options are JavaScript ones.
 *
 * Patterns written for Stylelint use lookahead, lookbehind and
 * backreferences, so they must work. One that does not compile at all is
 * reported the way Stylelint reports an invalid option (an
 * `invalidOptionWarnings` entry, `errored`, exit 2, and `Invalid Option:` once
 * per run in the text output) rather than switching the rule off silently.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

describe("a JavaScript pattern", () => {
  test("with a lookahead works in selector-class-pattern", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "selector-class-pattern": "^(?!js-)[a-z-]+$" }),
      "a.css": ".card {}\n.js-card {}\n",
    });

    const result = runGaleJson(["a.css"], { cwd: project.dir });

    expect(result.json()[0].invalidOptionWarnings).toEqual([]);
    const warnings = result.warnings();
    expect(warnings.map((w) => [w.rule, w.line])).toEqual([["selector-class-pattern", 2]]);
    expect(warnings[0].text).toContain(".js-card");
  });

  test("written as a /regex/i literal applies its flags", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "selector-class-pattern": "/^[a-z]+$/i" }),
      "a.css": ".Card {}\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([]);
  });
});

describe("a pattern that does not compile", () => {
  const message =
    'Invalid option value "^[a-z" for rule "selector-class-pattern": not a valid regular expression';

  function project() {
    return makeProject({
      ".stylelintrc.json": config({ "selector-class-pattern": "^[a-z", "block-no-empty": true }),
      "a.css": ".A {}\n",
      "b.css": ".B { color: red; }\n",
    });
  }

  test("is an invalid option on each result, which fails the run", () => {
    const result = runGaleJson(["a.css", "b.css"], { cwd: project().dir });

    expect(result.exitCode).toBe(2);
    for (const source of result.json()) {
      expect(source.errored).toBe(true);
      expect(source.invalidOptionWarnings).toHaveLength(1);
      expect((source.invalidOptionWarnings[0] as { text: string }).text).toStartWith(message);
    }
    // The broken rule reports nothing; the others still run.
    expect(result.warnings().map((w) => w.rule)).toEqual(["block-no-empty"]);
  });

  test("is listed once in the text output however many files it applies to", () => {
    const result = runGale(["--no-color", "a.css", "b.css"], { cwd: project().dir });

    expect(result.exitCode).toBe(2);
    expect(result.stdout).toStartWith(`Invalid Option: ${message}`);
    expect(result.stdout.split("Invalid Option:")).toHaveLength(2);
  });

  test("fails the run even when there are no other problems", () => {
    const quiet = makeProject({
      ".stylelintrc.json": config({ "selector-class-pattern": "^[a-z" }),
      "a.css": ".a {}\n",
    });

    expect(runGale(["a.css"], { cwd: quiet.dir }).exitCode).toBe(2);
  });

  test("in an ignore list is reported too, and the rule keeps running", () => {
    const listed = makeProject({
      ".stylelintrc.json": config({
        "color-named": ["never", { ignoreProperties: ["/[broken/"] }],
      }),
      "a.css": "a { color: red; }\n",
    });

    const [source] = runGaleJson(["a.css"], { cwd: listed.dir }).json();
    expect(source.invalidOptionWarnings).toHaveLength(1);
    expect((source.invalidOptionWarnings[0] as { text: string }).text).toStartWith(
      'Invalid option value "/[broken/" for rule "color-named"',
    );
    expect(source.warnings.map((w) => w.rule)).toEqual(["color-named"]);
  });
});
