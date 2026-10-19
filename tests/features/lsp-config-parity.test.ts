/**
 * The language server reports what the CLI reports.
 *
 * Both resolve a file's rules through the same layer, so a file opened in an
 * editor gets the same problems `gale <file>` prints: the closest config,
 * `overrides` for that file, `ignoreFiles`, `customSyntax`, and the config's
 * disable-comment settings.
 */

import { afterAll, afterEach, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGaleJson, type Project } from "./helpers";
import { LspClient } from "./lsp-client";

afterAll(cleanupProjects);

let client: LspClient | undefined;

afterEach(async () => {
  await client?.close();
  client = undefined;
});

/** The rule codes the server publishes for `rel` in `project`. */
async function lspRules(project: Project, rel: string, languageId = "css"): Promise<string[]> {
  client = new LspClient(project.dir);
  await client.initialize(project.dir);
  const diagnostics = (await client.open(
    `file://${project.path(rel)}`,
    project.read(rel),
    languageId,
  )) as { code?: string }[];
  return diagnostics.map((d) => String(d.code)).sort();
}

/** The rules the CLI reports for `rel` in `project`. */
function cliRules(project: Project, rel: string): string[] {
  return runGaleJson([rel], { cwd: project.dir })
    .warnings()
    .map((w) => w.rule)
    .sort();
}

describe("LSP and CLI agree", () => {
  test("on a file under the root config", async () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": "a {}\n",
    });
    expect(await lspRules(project, "a.css")).toEqual(cliRules(project, "a.css"));
  });

  test("on a file an override turns a rule on for", async () => {
    const project = makeProject({
      ".stylelintrc.json": config(
        { "block-no-empty": true },
        { overrides: [{ files: ["*.scss"], rules: { "color-hex-case": "lower" } }] },
      ),
      "a.scss": "a {}\nb { color: #FFF; }\n",
    });
    const expected = cliRules(project, "a.scss");
    expect(expected).toEqual(["block-no-empty", "color-hex-case"]);
    expect(await lspRules(project, "a.scss", "scss")).toEqual(expected);
  });

  test("on a file whose closest config is in its own directory", async () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "nested/.stylelintrc.json": config({ "color-hex-case": "lower" }),
      "nested/a.css": "a {}\nb { color: #FFF; }\n",
    });
    const expected = cliRules(project, "nested/a.css");
    expect(expected).toEqual(["color-hex-case"]);
    expect(await lspRules(project, "nested/a.css")).toEqual(expected);
  });

  test("on a file the config ignores", async () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { ignoreFiles: ["vendor/**"] }),
      "vendor/a.css": "a {}\n",
    });
    expect(await lspRules(project, "vendor/a.css")).toEqual([]);
  });

  test("on a file an override ignores", async () => {
    const project = makeProject({
      ".stylelintrc.json": config(
        { "block-no-empty": true },
        { overrides: [{ files: ["legacy/**"], ignoreFiles: ["legacy/old.css"], rules: {} }] },
      ),
      "legacy/old.css": "a {}\n",
    });
    expect(cliRules(project, "legacy/old.css")).toEqual([]);
    expect(await lspRules(project, "legacy/old.css")).toEqual([]);
  });

  test("on a file whose customSyntax gale cannot parse", async () => {
    const project = makeProject({
      ".stylelintrc.json": config(
        { "block-no-empty": true },
        { overrides: [{ files: ["lit/**"], customSyntax: "postcss-lit" }] },
      ),
      "lit/a.css": "a {}\n",
    });
    expect(cliRules(project, "lit/a.css")).toEqual([]);
    expect(await lspRules(project, "lit/a.css")).toEqual([]);
  });

  test("on the config's disable-comment reports", async () => {
    const project = makeProject({
      ".stylelintrc.json": config(
        { "block-no-empty": true },
        { reportDescriptionlessDisables: true },
      ),
      "a.css": [
        "/* stylelint-disable-next-line block-no-empty */",
        "a {}",
        "/* stylelint-disable-next-line block-no-empty */",
        "b { top: 0; }",
        "",
      ].join("\n"),
    });
    const expected = cliRules(project, "a.css");
    expect(expected).toEqual([
      "--report-descriptionless-disables",
      "--report-descriptionless-disables",
    ]);
    expect(await lspRules(project, "a.css")).toEqual(expected);
  });

  test("on the config's ignoreDisables", async () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { ignoreDisables: true }),
      "a.css": "/* stylelint-disable block-no-empty */\na {}\n",
    });
    const expected = cliRules(project, "a.css");
    expect(expected).toEqual(["block-no-empty"]);
    expect(await lspRules(project, "a.css")).toEqual(expected);
  });
});
