/**
 * Styles embedded in Vue, Svelte, Astro and HTML files.
 *
 * Stylelint lints these files with `customSyntax: "postcss-html"`: every
 * `<style>` element and `style="…"` attribute is linted as a style sheet of
 * its own, and problems are reported at their line and column in the file.
 * The expected positions below are the ones Stylelint 17 with postcss-html
 * reports for the same input.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

const RULES = {
  "block-no-empty": true,
  "color-hex-length": "short",
  "length-zero-no-unit": true,
  "no-duplicate-selectors": true,
};

/** Each warning as `line:column rule`, in output order. */
function positions(result: ReturnType<typeof runGale>): string[] {
  return result.warnings().map((w) => `${w.line}:${w.column} ${w.rule}`);
}

const VUE = `<template>
  <div class="a" style="margin: 0px">{{ msg }}</div>
</template>

<script setup>
const s = '<style>.x {}</style>'
</script>

<style scoped>
.a {}
.b { color: #ffffff; }
</style>

<style lang="scss">
$c: #ffffff;
.c { margin: 0px; }
</style>
`;

describe("embedded style blocks", () => {
  test("a Vue file is linted block by block at host positions", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "App.vue": VUE });

    const result = runGaleJson(["App.vue"], { cwd: project.dir });

    expect(result.exitCode).toBe(2);
    expect(result.stderr).toBe("");
    expect(positions(result)).toEqual([
      "2:34 length-zero-no-unit",
      "10:4 block-no-empty",
      "11:13 color-hex-length",
      "15:5 color-hex-length",
      "16:15 length-zero-no-unit",
    ]);
  });

  test("Svelte, Astro and HTML files are linted too", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "App.svelte":
        '<script>\n  let n = 0;\n</script>\n\n<p style="margin: 0px">{n > 1 ? "big" : ""}</p>\n\n' +
        "<style>\n  p {\n    color: #ffffff;\n  }\n</style>\n",
      "Page.astro":
        '---\nconst css = "<style>.front {}</style>";\n---\n\n<h1>Hi</h1>\n\n' +
        "<style is:global>\n  h1 { margin: 0px; }\n</style>\n",
      "index.html":
        "<!doctype html>\n<html>\n  <head>\n    <style>\n      body { color: #ffffff; }\n    </style>\n" +
        '  </head>\n  <body>\n    <!-- <style>.hidden {}</style> -->\n    <p style="margin: 0px">x</p>\n' +
        "  </body>\n</html>\n",
    });

    const result = runGaleJson(["App.svelte", "Page.astro", "index.html"], { cwd: project.dir });

    const bySource = Object.fromEntries(
      result.json().map((r) => [r.source, r.warnings.map((w) => `${w.line}:${w.column} ${w.rule}`)]),
    );
    expect(bySource).toEqual({
      "App.svelte": ["5:20 length-zero-no-unit", "9:12 color-hex-length"],
      "Page.astro": ["8:17 length-zero-no-unit"],
      "index.html": ["5:21 color-hex-length", "10:24 length-zero-no-unit"],
    });
  });

  test("a '<style>' string in a Svelte expression does not open a block", () => {
    // postcss-html reads it as a tag and fails with a CssSyntaxError.
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "App.svelte": '<p>{n > 1 ? "<style>" : ""}</p>\n<style>\n  p { color: #ffffff; }\n</style>\n',
    });

    const result = runGaleJson(["App.svelte"], { cwd: project.dir });
    expect(positions(result)).toEqual(["3:14 color-hex-length"]);
  });

  test("an open stylelint-disable reaches later blocks; duplicates stay per block", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "Spans.vue":
        "<style>\n/* stylelint-disable block-no-empty */\n.a {}\n</style>\n\n" +
        "<style>\n.b {}\n.c { color: #ffffff; }\n</style>\n\n" +
        "<style>\n.a { color: red; }\n.a { color: blue; }\n</style>\n",
    });

    const result = runGaleJson(["Spans.vue"], { cwd: project.dir });

    expect(positions(result)).toEqual(["8:13 color-hex-length", "13:1 no-duplicate-selectors"]);
    // The line it names is the file's, not the block's.
    expect(result.warnings()[1].text).toContain("first used at line 12");
  });

  test("a customSyntax of postcss-html is accepted, not skipped", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, {
        overrides: [{ files: ["**/*.vue"], customSyntax: "postcss-html" }],
      }),
      "src/App.vue": VUE,
    });

    const result = runGaleJson(["src/**/*.vue"], { cwd: project.dir });

    expect(result.stderr).toBe("");
    expect(result.warnings()).toHaveLength(5);
  });

  test("a block in a language gale cannot parse is skipped with a warning", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "App.vue": '<style lang="stylus">\n.a\n  color red\n</style>\n<style>\n.b {}\n</style>\n',
    });

    const result = runGaleJson(["App.vue"], { cwd: project.dir });

    expect(positions(result)).toEqual(["6:4 block-no-empty"]);
    expect(result.stderr).toBe(
      "warning: Skipped 1 <style> block that gale cannot lint yet:\n" +
        '  App.vue (lang="stylus" is not supported)\n',
    );
  });

  test("a directory walk picks up HTML-like files", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "src/a.css": "a {}\n",
      "src/App.vue": "<style>\nb {}\n</style>\n",
      "src/notes.md": "# notes\n",
    });

    const result = runGaleJson(["src"], { cwd: project.dir });

    expect(result.json().map((r) => r.source).sort()).toEqual(["src/App.vue", "src/a.css"]);
  });
});

describe("--fix in embedded style blocks", () => {
  test("fixes are written back inside the blocks and nowhere else", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "Fix.vue":
        '<template><p style="margin: 0px">#ffffff 0px</p></template>\n' +
        "<script>\nexport default { data: () => ({ c: '#ffffff' }) }\n</script>\n" +
        "<style>\n.a { color: #ffffff; margin: 0px; }\n</style>\n",
    });

    const result = runGale(["--fix", "Fix.vue"], { cwd: project.dir });

    expect(result.exitCode).toBe(0);
    // The template text and the script look like CSS but are left alone.
    const after = project.read("Fix.vue");
    expect(after).toBe(
      '<template><p style="margin: 0">#ffffff 0px</p></template>\n' +
        "<script>\nexport default { data: () => ({ c: '#ffffff' }) }\n</script>\n" +
        "<style>\n.a { color: #fff; margin: 0; }\n</style>\n",
    );

    // A second run finds nothing left to fix and leaves the file alone.
    const again = runGale(["--fix", "Fix.vue"], { cwd: project.dir });
    expect(again.exitCode).toBe(0);
    expect(project.read("Fix.vue")).toBe(after);
  });

  test("stdin with a .vue filename prints the fixed file", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES) });
    const code = '<template><p style="margin: 0px">x</p></template>\n<style>\n.a { color: #ffffff; }\n</style>\n';

    const result = runGale(["--fix", "--stdin", "--stdin-filename", "Fix.vue"], {
      cwd: project.dir,
      stdin: code,
    });

    // Stylelint's own fix of the same input.
    expect(result.stdout).toBe(
      '<template><p style="margin: 0">x</p></template>\n<style>\n.a { color: #fff; }\n</style>\n',
    );
  });

  test("stdin with a .vue filename reports host positions", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES) });

    const result = runGaleJson(["--stdin", "--stdin-filename", "App.vue"], {
      cwd: project.dir,
      stdin: VUE,
    });

    expect(positions(result)).toEqual([
      "2:34 length-zero-no-unit",
      "10:4 block-no-empty",
      "11:13 color-hex-length",
      "15:5 color-hex-length",
      "16:15 length-zero-no-unit",
    ]);
  });
});
