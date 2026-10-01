/**
 * One bad file must never kill the run.
 *
 * A rule that panics (a byte offset inside a multibyte character, say) is
 * reported as an "Internal error" problem on that file, naming the rule, and
 * everything else still lints. `GALE_DEBUG_PANIC=<rule>` makes that rule
 * panic on any file containing `gale-debug-panic`, standing in for a real
 * bug so the guard can be tested end to end.
 */

import { afterAll, afterEach, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";
import { LspClient } from "./lsp-client";

afterAll(cleanupProjects);

const PANIC = { GALE_DEBUG_PANIC: "color-named" };

function crashingProject() {
  return makeProject({
    ".stylelintrc.json": config({ "block-no-empty": true, "color-named": "never" }),
    // color-named panics here; block-no-empty still has something to report.
    "bad.css": "/* gale-debug-panic */\na { color: red; }\nb {}\n",
    "good.css": "a { color: blue; }\nc {}\n",
  });
}

describe("a rule that panics", () => {
  test("is reported on its file and every other file still lints", () => {
    const project = crashingProject();

    const result = runGaleJson(["bad.css", "good.css"], { cwd: project.dir, env: PANIC });
    const [bad, good] = result.json();

    expect(bad.source).toBe("bad.css");
    const crash = bad.warnings.find((w) => w.rule === "color-named");
    expect(crash?.severity).toBe("error");
    expect(crash?.text).toStartWith('Internal error in rule "color-named": ');
    expect(crash?.text).toContain("GALE_DEBUG_PANIC: simulated crash in color-named");
    expect(crash?.text).toContain("please report it at https://github.com/codebend3r/gale/issues");
    // The other rule still reports on the same file.
    expect(bad.warnings.map((w) => w.rule)).toContain("block-no-empty");

    // The other file is linted as usual, including by the crashing rule.
    expect(good.source).toBe("good.css");
    expect(good.warnings.map((w) => w.rule).sort()).toEqual(["block-no-empty", "color-named"]);
    expect(good.warnings.every((w) => !w.text.startsWith("Internal error"))).toBe(true);
  });

  test("makes the run exit non-zero", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "color-named": ["never", { severity: "warning" }] }),
      "bad.css": "/* gale-debug-panic */\na { color: red; }\n",
    });

    // The crash is an error even though the rule is set to warn.
    expect(runGale(["bad.css"], { cwd: project.dir, env: PANIC }).exitCode).toBe(2);
  });

  test("leaves no panic message or backtrace hint on stderr", () => {
    const project = crashingProject();

    const result = runGale(["bad.css", "good.css"], { cwd: project.dir, env: PANIC });

    expect(result.stderr).not.toContain("panicked at");
    expect(result.stderr).not.toContain("RUST_BACKTRACE");
    expect(result.stdout).toContain("Internal error in rule");
  });

  test("is not hidden by a disable comment", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "color-named": "never" }),
      "bad.css": "/* stylelint-disable */\n/* gale-debug-panic */\na { color: red; }\n",
    });

    const warnings = runGaleJson(["bad.css"], { cwd: project.dir, env: PANIC }).warnings();
    expect(warnings.map((w) => w.rule)).toEqual(["color-named"]);
    expect(warnings[0].text).toStartWith("Internal error");
  });
});

describe("a rule that panics under the language server", () => {
  let client: LspClient | undefined;

  afterEach(async () => {
    await client?.close();
    client = undefined;
  });

  interface Diagnostic {
    code?: string;
    message: string;
  }

  test("is published as a diagnostic and the server keeps serving", async () => {
    const project = crashingProject();
    client = new LspClient(project.dir, [], PANIC);
    await client.initialize(project.dir);

    const bad = (await client.open(
      `file://${project.path("bad.css")}`,
      project.read("bad.css"),
    )) as Diagnostic[];
    const crash = bad.find((d) => d.code === "color-named");
    expect(crash?.message).toStartWith('Internal error in rule "color-named"');
    expect(bad.map((d) => d.code)).toContain("block-no-empty");

    // The server survived and lints the next document normally.
    const good = (await client.open(
      `file://${project.path("good.css")}`,
      project.read("good.css"),
    )) as Diagnostic[];
    expect(good.map((d) => d.code).sort()).toEqual(["block-no-empty", "color-named"]);
  });
});
