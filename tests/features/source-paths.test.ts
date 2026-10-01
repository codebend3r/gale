/**
 * The path each result is reported under.
 *
 * Stylelint reports every source by its absolute path, however the file was
 * named: a path, a glob, a directory, with or without `./` or `..`.  A
 * `--stdin-filename` (the Node API's `codeFilename`) is resolved from the
 * working directory too.  Its string formatter names files relative to the
 * working directory; the compact, unix and tap formatters print the source
 * as it is.  These are the paths Stylelint 17 reports for the same input.
 */

import { realpathSync } from "node:fs";
import { join } from "node:path";

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, reportedPath, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

function project() {
  return makeProject({
    ".stylelintrc.json": config({ "block-no-empty": true }),
    "src/a.css": "a {}\n",
    "src/sub/b.css": "b {}\n",
  });
}

/** The `source` of every result, sorted. */
function sources(args: string[], cwd: string, stdin?: string): string[] {
  return runGaleJson(args, { cwd, stdin })
    .json()
    .map((r) => r.source)
    .sort();
}

describe("JSON sources are absolute", () => {
  const both = ["src/a.css", "src/sub/b.css"];

  test.each([
    ["a path", ["src/a.css"], ["src/a.css"]],
    ["a ./ path", ["./src/a.css"], ["src/a.css"]],
    ["a path through ..", ["src/sub/../a.css"], ["src/a.css"]],
    ["a glob", ["src/*.css"], ["src/a.css"]],
    ["a ./ glob", ["./src/*.css"], ["src/a.css"]],
    ["a recursive glob", ["src/**/*.css"], both],
    ["a ./ recursive glob", ["./src/**/*.css"], both],
    ["a directory", ["src"], both],
    ["a ./ directory", ["./src"], both],
    ["the working directory", ["."], both],
  ])("%s", (_name, args, expected) => {
    const { dir } = project();
    expect(sources(args, dir)).toEqual(expected.map((rel) => reportedPath(dir, rel)));
  });

  test("an absolute path or glob", () => {
    const { dir } = project();
    const real = realpathSync(dir);
    expect(sources([join(real, "src/a.css")], dir)).toEqual([join(real, "src/a.css")]);
    expect(sources([join(real, "src/**/*.css")], dir)).toEqual([
      join(real, "src/a.css"),
      join(real, "src/sub/b.css"),
    ]);
  });

  test("a path that climbs out of the working directory", () => {
    const { dir } = project();
    expect(sources(["../src/a.css"], join(dir, "src"))).toEqual([reportedPath(dir, "src/a.css")]);
  });
});

describe("--stdin-filename", () => {
  test("a relative name is resolved from the working directory", () => {
    const { dir } = project();
    expect(sources(["--stdin", "--stdin-filename", "./src/x.css"], dir, "a {}\n")).toEqual([
      reportedPath(dir, "src/x.css"),
    ]);
  });

  test("an absolute name is kept as given", () => {
    const { dir } = project();
    expect(sources(["--stdin", "--stdin-filename", "/elsewhere/./x.css"], dir, "a {}\n")).toEqual([
      "/elsewhere/./x.css",
    ]);
  });

  test("without one, stdin keeps its default name", () => {
    const { dir } = project();
    expect(sources(["--stdin"], dir, "a {}\n")).toEqual(["stdin.css"]);
  });
});

describe("formatters", () => {
  test("the string formatter names files relative to the working directory", () => {
    const { dir } = project();
    const output = runGale(["--formatter", "string", "--no-color", "./src/**/*.css"], { cwd: dir }).stdout;
    const headers = output.split("\n").filter((line) => line.endsWith(".css"));
    expect(headers).toEqual(["src/a.css", "src/sub/b.css"]);
  });

  test("the unix formatter prints the absolute path", () => {
    const { dir } = project();
    const output = runGale(["--formatter", "unix", "./src/a.css"], { cwd: dir }).stdout;
    expect(output).toStartWith(`${reportedPath(dir, "src/a.css")}:1:3: `);
  });
});
