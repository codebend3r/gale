/**
 * The language server reports what the CLI reports.
 *
 * Both build their lint runner from the same config, so a file opened in an
 * editor gets the same problems `gale <file>` prints: the closest config,
 * `overrides` for that file, and `ignoreFiles`.
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

  test.failing("on a file an override turns a rule on for", async () => {
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

  test.failing("on a file whose closest config is in its own directory", async () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "nested/.stylelintrc.json": config({ "color-hex-case": "lower" }),
      "nested/a.css": "a {}\nb { color: #FFF; }\n",
    });
    const expected = cliRules(project, "nested/a.css");
    expect(expected).toEqual(["color-hex-case"]);
    expect(await lspRules(project, "nested/a.css")).toEqual(expected);
  });

  test.failing("on a file the config ignores", async () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { ignoreFiles: ["vendor/**"] }),
      "vendor/a.css": "a {}\n",
    });
    expect(await lspRules(project, "vendor/a.css")).toEqual([]);
  });
});
