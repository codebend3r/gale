/**
 * Reports about `stylelint-disable` comments themselves: needless, invalid
 * scope, descriptionless, unscoped, and the per-rule `reportDisables` option.
 *
 * Stylelint emits these through `reportCommentProblem`, which does not append
 * the ` (rule-name)` suffix that ordinary rule warnings carry. The expected
 * texts below are exact.
 *
 * Tiers 1, 2 and 3.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

const RULES = { "block-no-empty": true };

// ---------------------------------------------------------------------------
// Tier 1
// ---------------------------------------------------------------------------

describe("[tier 1] --report-needless-disables text", () => {
  // `material/no-prefixes` is a registered no-op, so a disable for it is the
  // one case Gale can prove needless.
  const rules = { "block-no-empty": true, "material/no-prefixes": true };
  const source = "/* stylelint-disable material/no-prefixes */\na { color: red; }\n";

  test.failing("matches Stylelint's comment-problem wording exactly", () => {
    const project = makeProject({ ".stylelintrc.json": config(rules), "a.css": source });

    const warnings = runGaleJson(["--report-needless-disables", "a.css"], {
      cwd: project.dir,
    }).warnings();

    expect(warnings).toEqual([
      expect.objectContaining({
        rule: "--report-needless-disables",
        severity: "error",
        text: 'Needless disable for "material/no-prefixes"',
        line: 1,
        column: 1,
      }),
    ]);
  });
});

describe("[tier 1] reportInvalidScopeDisables", () => {
  const source = "/* stylelint-disable color-named */\na { color: red; }\n";
  const expected = expect.objectContaining({
    rule: "--report-invalid-scope-disables",
    severity: "error",
    text: 'Rule "color-named" isn\'t enabled',
    line: 1,
    column: 1,
  });

  test("nothing is reported when the option is off", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": source });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test.failing("the CLI flag reports a disable for a rule that is not configured", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": source });

    const warnings = runGaleJson(["--report-invalid-scope-disables", "a.css"], {
      cwd: project.dir,
    }).warnings();

    expect(warnings).toEqual([expected]);
  });

  test.failing("the config key reports a disable for a rule that is not configured", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportInvalidScopeDisables: true }),
      "a.css": source,
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([expected]);
  });

  test("a disable for a configured rule is in scope", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportInvalidScopeDisables: true }),
      "a.css": "/* stylelint-disable block-no-empty */\na { color: red; }\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test("an unscoped disable is never an invalid scope", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportInvalidScopeDisables: true }),
      "a.css": "/* stylelint-disable */\na { color: red; }\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test.failing("disable-next-line comments are checked too", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportInvalidScopeDisables: true }),
      "a.css": "/* stylelint-disable-next-line color-named */\na { color: red; }\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([expected]);
  });
});

describe("[tier 1] reportDescriptionlessDisables", () => {
  const undescribed = "/* stylelint-disable block-no-empty */\na {}\n";
  const described = "/* stylelint-disable block-no-empty -- legacy markup */\na {}\n";
  const expected = expect.objectContaining({
    rule: "--report-descriptionless-disables",
    severity: "error",
    text: 'Disable for "block-no-empty" is missing a description',
    line: 1,
    column: 1,
  });

  test("nothing is reported when the option is off", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": undescribed });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test.failing("the CLI flag reports a disable with no description", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": undescribed });

    const warnings = runGaleJson(["--report-descriptionless-disables", "a.css"], {
      cwd: project.dir,
    }).warnings();

    expect(warnings).toEqual([expected]);
  });

  test.failing("the config key reports a disable with no description", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportDescriptionlessDisables: true }),
      "a.css": undescribed,
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([expected]);
  });

  test("a description after -- satisfies the check", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportDescriptionlessDisables: true }),
      "a.css": described,
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test.failing('an unscoped disable is reported as "all"', () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportDescriptionlessDisables: true }),
      "a.css": "/* stylelint-disable */\na {}\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([
      expect.objectContaining({
        rule: "--report-descriptionless-disables",
        text: 'Disable for "all" is missing a description',
      }),
    ]);
  });

  test.failing("disable-next-line comments are checked too", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportDescriptionlessDisables: true }),
      "a.css": "/* stylelint-disable-next-line block-no-empty */\na {}\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([expected]);
  });
});

// ---------------------------------------------------------------------------
// Tier 2
// ---------------------------------------------------------------------------

describe("[tier 2] reportUnscopedDisables", () => {
  const unscoped = "/* stylelint-disable */\na {}\n";
  const expected = expect.objectContaining({
    rule: "--report-unscoped-disables",
    severity: "error",
    text: "Configuration comment must be scoped",
    line: 1,
    column: 1,
  });

  test("nothing is reported when the option is off", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": unscoped });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test.failing("the CLI flag reports a disable that names no rule", () => {
    const project = makeProject({ ".stylelintrc.json": config(RULES), "a.css": unscoped });

    const warnings = runGaleJson(["--report-unscoped-disables", "a.css"], {
      cwd: project.dir,
    }).warnings();

    expect(warnings).toEqual([expected]);
  });

  test.failing("the config key reports a disable that names no rule", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportUnscopedDisables: true }),
      "a.css": unscoped,
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([expected]);
  });

  test("a scoped disable is fine", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES, { reportUnscopedDisables: true }),
      "a.css": "/* stylelint-disable block-no-empty */\na {}\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test.failing("the flag is accepted and does nothing when no comments are present", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "a.css": "a { color: red; }\n",
    });

    const result = runGaleJson(["--report-unscoped-disables", "a.css"], { cwd: project.dir });
    expect(result.exitCode).toBe(0);
    expect(result.warnings()).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// Tier 3
// ---------------------------------------------------------------------------

describe("[tier 3] reportDisables secondary option", () => {
  const rules = { "block-no-empty": [true, { reportDisables: true }] };
  const expected = expect.objectContaining({
    rule: "reportDisables",
    severity: "error",
    text: 'Rule "block-no-empty" may not be disabled',
    line: 1,
    column: 1,
  });

  test("a disable is silent without the option", () => {
    const project = makeProject({
      ".stylelintrc.json": config(RULES),
      "a.css": "/* stylelint-disable block-no-empty */\na {}\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });

  test.failing("a range disable for the rule is reported", () => {
    const project = makeProject({
      ".stylelintrc.json": config(rules),
      "a.css": "/* stylelint-disable block-no-empty */\na {}\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([expected]);
  });

  test.failing("a disable-next-line for the rule is reported", () => {
    const project = makeProject({
      ".stylelintrc.json": config(rules),
      "a.css": "/* stylelint-disable-next-line block-no-empty */\na {}\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toEqual([expected]);
  });

  test("a disable for a different rule is not reported", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ ...rules, "color-named": "never" }),
      "a.css": "/* stylelint-disable color-named */\na { color: red; }\n",
    });

    expect(runGaleJson(["a.css"], { cwd: project.dir }).warnings()).toHaveLength(0);
  });
});
