/**
 * Secondary options every Stylelint rule accepts: `message`, `url`, and
 * `disableFix`. (`severity` already works; `reportDisables` is in
 * disable-reports.test.ts.)
 */

import { afterAll, describe, expect, test } from "bun:test";

import { EMPTY_BLOCK, UPPER_HEX, cleanupProjects, config, makeProject, runGale, runGaleJson } from "./helpers";

afterAll(cleanupProjects);

describe("message secondary option", () => {
  test("pattern rules already honour a custom message", () => {
    const project = makeProject({
      ".stylelintrc.json": config({
        "custom-property-pattern": ["^foo", { message: "Bad custom property" }],
      }),
      "a.css": ":root { --bar: 1px; }\n",
    });

    const [warning] = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect(warning.text).toBe("Bad custom property (custom-property-pattern)");
  });

  test.failing("a boolean-primary rule uses the custom message", () => {
    const project = makeProject({
      ".stylelintrc.json": config({
        "block-no-empty": [true, { message: "No empty blocks please" }],
      }),
      "a.css": EMPTY_BLOCK,
    });

    const [warning] = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect(warning.text).toBe("No empty blocks please (block-no-empty)");
    expect(warning.rule).toBe("block-no-empty");
  });

  test.failing("a string-primary rule uses the custom message", () => {
    const project = makeProject({
      ".stylelintrc.json": config({
        "color-hex-case": ["lower", { message: "Lowercase hex only" }],
      }),
      "a.css": UPPER_HEX,
    });

    const [warning] = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect(warning.text).toBe("Lowercase hex only (color-hex-case)");
  });

  test.failing("the custom message still respects severity", () => {
    const project = makeProject({
      ".stylelintrc.json": config({
        "block-no-empty": [true, { message: "Custom", severity: "warning" }],
      }),
      "a.css": EMPTY_BLOCK,
    });

    const [warning] = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect(warning.text).toBe("Custom (block-no-empty)");
    expect(warning.severity).toBe("warning");
  });
});

describe("url secondary option", () => {
  test("no url key is present by default", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      "a.css": EMPTY_BLOCK,
    });

    const [warning] = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect("url" in warning).toBe(false);
  });

  test.failing("the configured url is attached to every warning of the rule", () => {
    const project = makeProject({
      ".stylelintrc.json": config({
        "block-no-empty": [true, { url: "https://example.com/rules/block-no-empty" }],
      }),
      "a.css": "a {}\nb {}\n",
    });

    const warnings = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    expect(warnings).toHaveLength(2);
    for (const warning of warnings) {
      expect(warning.url).toBe("https://example.com/rules/block-no-empty");
    }
  });

  test.failing("other rules do not inherit the url", () => {
    const project = makeProject({
      ".stylelintrc.json": config({
        "block-no-empty": [true, { url: "https://example.com/empty" }],
        "color-hex-case": "lower",
      }),
      "a.css": "a {}\nb { color: #FFF; }\n",
    });

    const warnings = runGaleJson(["a.css"], { cwd: project.dir }).warnings();
    const byRule = Object.fromEntries(warnings.map((w) => [w.rule, w]));
    expect(byRule["block-no-empty"].url).toBe("https://example.com/empty");
    expect("url" in byRule["color-hex-case"]).toBe(false);
  });
});

describe("disableFix secondary option", () => {
  test("--fix rewrites the file without the option", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "color-hex-case": "lower" }),
      "a.css": UPPER_HEX,
    });

    runGale(["--fix", "a.css"], { cwd: project.dir });
    expect(project.read("a.css")).toBe("a { color: #fff; }\n");
  });

  test.failing("the rule is still reported but never fixed", () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "color-hex-case": ["lower", { disableFix: true }] }),
      "a.css": UPPER_HEX,
    });

    const result = runGaleJson(["--fix", "a.css"], { cwd: project.dir });
    expect(project.read("a.css")).toBe(UPPER_HEX);
    expect(result.warnings().map((w) => w.rule)).toEqual(["color-hex-case"]);
  });

  test.failing("other rules keep fixing", () => {
    const project = makeProject({
      ".stylelintrc.json": config({
        "color-hex-case": ["lower", { disableFix: true }],
        "length-zero-no-unit": true,
      }),
      "a.css": "a { color: #FFF; margin: 0px; }\n",
    });

    runGale(["--fix", "a.css"], { cwd: project.dir });
    expect(project.read("a.css")).toBe("a { color: #FFF; margin: 0; }\n");
  });
});
