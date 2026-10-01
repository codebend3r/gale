/**
 * Columns after characters outside ASCII.
 *
 * Stylelint's columns are JavaScript string indices, which count UTF-16
 * code units: an emoji takes two columns, `é` or `中` one, whatever their
 * size in UTF-8.  PostCSS drops a byte order mark before counting.  The
 * expected positions below are the ones Stylelint 17 reports for the same
 * input.
 */

import { afterAll, afterEach, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";
import { LspClient } from "./lsp-client";

afterAll(cleanupProjects);

const RULES = { "block-no-empty": true, "length-zero-no-unit": true };

/** Each warning as `line:column-endLine:endColumn rule`. */
function positions(files: Record<string, string>, file: string): string[] {
  const project = makeProject({ ".stylelintrc.json": config(RULES), ...files });
  return runGaleJson([file], { cwd: project.dir })
    .warnings()
    .map((w) => `${w.line}:${w.column}-${w.endLine}:${w.endColumn} ${w.rule}`);
}

describe("columns count UTF-16 code units", () => {
  test("an emoji before the problem takes two columns", () => {
    expect(positions({ "a.css": 'a::before { content: "😀"; } b {}\n' }, "a.css")).toEqual([
      "1:32-1:34 block-no-empty",
    ]);
  });

  test("an accented letter takes one", () => {
    expect(positions({ "a.css": 'a::before { content: "é"; } b {}\n' }, "a.css")).toEqual([
      "1:31-1:33 block-no-empty",
    ]);
  });

  test("a byte order mark takes none", () => {
    expect(positions({ "a.css": "﻿a {}\n" }, "a.css")).toEqual(["1:3-1:5 block-no-empty"]);
  });

  test("the string formatter agrees", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "a.css": 'a::before { content: "😀"; } b {}\n',
    });
    const output = runGale(["--formatter", "string", "--no-color", "a.css"], { cwd: project.dir }).stdout;
    expect(output).toContain("1:32");
  });

  test("embedded styles count the host file's characters", () => {
    const vue = "<p>😀</p><style>a {}</style>\n<p style=\"content: '😀'; margin: 0px\"></p>\n";
    expect(positions({ "App.vue": vue }, "App.vue")).toEqual([
      "1:19-1:21 block-no-empty",
      "2:35-2:37 length-zero-no-unit",
    ]);
  });

  test("Sass counts the characters of the Sass file", () => {
    const sass = '.a\n  margin: 1px "😀" 0px\n';
    expect(positions({ "a.sass": sass }, "a.sass")).toEqual(["2:21-2:23 length-zero-no-unit"]);
  });
});

describe("the language server", () => {
  let client: LspClient | undefined;

  afterEach(async () => {
    await client?.close();
    client = undefined;
  });

  test("reports UTF-16 characters, counted once", async () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "a.css": 'a::before { content: "😀"; } b {}\n',
    });
    client = new LspClient(project.dir);
    await client.initialize(project.dir);

    const uri = `file://${project.path("a.css")}`;
    const diagnostics = (await client.open(uri, project.read("a.css"))) as {
      range: { start: { line: number; character: number }; end: { line: number; character: number } };
    }[];

    expect(diagnostics.map((d) => d.range)).toEqual([
      { start: { line: 0, character: 31 }, end: { line: 0, character: 33 } },
    ]);
  });
});
