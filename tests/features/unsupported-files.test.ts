/**
 * Files gale cannot lint yet are skipped out loud.
 *
 * Styles in Vue, Svelte, HTML, Astro, Markdown or JavaScript files, and files
 * whose `customSyntax` gale cannot parse, are left out of the run with one
 * stderr warning naming them. They still count as input: matching only such
 * files is not "no files matching", and `allowEmptyInput` is only about
 * patterns that match nothing at all.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { EMPTY_BLOCK, cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

const VUE = "<template/>\n<style>\na { colr: red; }\n</style>\n";

describe("a file type gale cannot lint yet", () => {
  test("named on the command line is skipped with a warning, not 'no files'", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "b.vue": VUE,
    });

    const result = runGale(["b.vue"], { cwd: project.dir });

    expect(result.exitCode).toBe(0);
    expect(result.stderr).not.toContain("No files matching");
    expect(result.stderr).toBe(
      "warning: Skipped 1 file that gale cannot lint yet:\n  b.vue (Vue files are not supported)\n",
    );
  });

  test("matched by a glob is skipped while the CSS beside it is linted", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "src/a.css": EMPTY_BLOCK,
      "src/b.vue": VUE,
      "src/c.svelte": VUE,
    });

    const result = runGaleJson(["src/*.{css,vue,svelte}"], { cwd: project.dir });

    expect(result.exitCode).toBe(2);
    expect(result.json().map((r) => r.source)).toEqual(["src/a.css"]);
    const warning = result.stderr.split("\n");
    expect(warning[0]).toBe("warning: Skipped 2 files that gale cannot lint yet:");
    expect(warning).toContain("  src/c.svelte (Svelte files are not supported)");
    expect(warning).toContain("  src/b.vue (Vue files are not supported)");
  });

  test("is still reported as 'no files' when nothing matches at all", () => {
    const project = makeProject({ ".stylelintrc.json": config({ "block-no-empty": true }) });

    const result = runGale(["missing.vue"], { cwd: project.dir });
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain('No files matching the pattern "missing.vue" were found.');
  });
});

describe("a customSyntax gale cannot parse", () => {
  test("skips the files it covers with a warning", () => {
    const project = makeProject({
      ".stylelintrc.json": config(
        { "block-no-empty": true },
        { overrides: [{ files: ["**/*.lit.css"], customSyntax: "postcss-lit" }] },
      ),
      "a.css": EMPTY_BLOCK,
      "b.lit.css": EMPTY_BLOCK,
    });

    const result = runGaleJson(["a.css", "b.lit.css"], { cwd: project.dir });

    expect(result.json().map((r) => r.source)).toEqual(["a.css"]);
    expect(result.stderr).toContain(
      '  b.lit.css (customSyntax "postcss-lit" is not supported)',
    );
  });
});
