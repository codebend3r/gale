/**
 * Quick-fix code actions from the language server.
 */

import { afterAll, afterEach, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject } from "./helpers";
import { LspClient } from "./lsp-client";

afterAll(cleanupProjects);

let client: LspClient | undefined;

afterEach(async () => {
  await client?.close();
  client = undefined;
});

interface Diagnostic {
  range: { start: { line: number; character: number }; end: { line: number; character: number } };
  code?: string;
  message: string;
}

interface CodeAction {
  title: string;
  kind?: string;
  diagnostics?: Diagnostic[];
  edit?: { changes?: Record<string, { range: Diagnostic["range"]; newText: string }[]> };
}

function fixableProject() {
  return makeProject({
    ".stylelintrc.json": config({ "color-hex-case": "lower", "block-no-empty": true }),
    "a.css": "a { color: #FFF; }\n",
  });
}

describe("LSP code actions", () => {
  test("the server publishes diagnostics for an opened document", async () => {
    const project = fixableProject();
    client = new LspClient(project.dir);

    const init = await client.initialize(project.dir);
    expect((init.serverInfo as { name: string }).name).toBe("gale-lsp");

    const uri = `file://${project.path("a.css")}`;
    const diagnostics = (await client.open(uri, project.read("a.css"))) as Diagnostic[];

    expect(diagnostics).toHaveLength(1);
    expect(diagnostics[0].code).toBe("color-hex-case");
  });

  test.failing("the server advertises code actions", async () => {
    const project = fixableProject();
    client = new LspClient(project.dir);

    const init = await client.initialize(project.dir);
    const capabilities = init.capabilities as { codeActionProvider?: unknown };

    expect(capabilities.codeActionProvider).toBeTruthy();
  });

  test.failing("a fixable diagnostic yields a quickfix with the edit", async () => {
    const project = fixableProject();
    client = new LspClient(project.dir);
    await client.initialize(project.dir);

    const uri = `file://${project.path("a.css")}`;
    const diagnostics = (await client.open(uri, project.read("a.css"))) as Diagnostic[];

    const actions = await client.request<CodeAction[]>("textDocument/codeAction", {
      textDocument: { uri },
      range: diagnostics[0].range,
      context: { diagnostics },
    });

    expect(actions.length).toBeGreaterThan(0);
    const quickfix = actions.find((a) => a.kind === "quickfix");
    expect(quickfix).toBeDefined();

    const edits = quickfix?.edit?.changes?.[uri] ?? [];
    expect(edits).toHaveLength(1);
    expect(edits[0].newText).toBe("#fff");
    expect(edits[0].range.start).toEqual({ line: 0, character: 11 });
    expect(edits[0].range.end).toEqual({ line: 0, character: 15 });
  });

  test.failing("a diagnostic without a fix yields no code action", async () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": "a {}\n",
    });
    client = new LspClient(project.dir);
    await client.initialize(project.dir);

    const uri = `file://${project.path("a.css")}`;
    const diagnostics = (await client.open(uri, project.read("a.css"))) as Diagnostic[];
    expect(diagnostics).toHaveLength(1);

    const actions = await client.request<CodeAction[]>("textDocument/codeAction", {
      textDocument: { uri },
      range: diagnostics[0].range,
      context: { diagnostics },
    });

    expect(actions).toEqual([]);
  });

  test.failing("code actions track the latest document text", async () => {
    const project = fixableProject();
    client = new LspClient(project.dir);
    await client.initialize(project.dir);

    const uri = `file://${project.path("a.css")}`;
    await client.open(uri, project.read("a.css"));

    const published = client.waitFor<{ diagnostics: Diagnostic[] }>(
      "textDocument/publishDiagnostics",
    );
    client.notify("textDocument/didChange", {
      textDocument: { uri, version: 2 },
      contentChanges: [{ text: "a { color: #fff; }\n" }],
    });
    expect((await published).diagnostics).toHaveLength(0);

    const actions = await client.request<CodeAction[]>("textDocument/codeAction", {
      textDocument: { uri },
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 18 } },
      context: { diagnostics: [] },
    });

    expect(actions).toEqual([]);
  });
});
