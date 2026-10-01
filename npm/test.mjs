#!/usr/bin/env node

/**
 * Basic smoke tests for the @codebend3r/gale programmatic API.
 *
 * Run: node test.mjs
 *
 * Requires a working gale binary (either in npm/bin/ or on PATH).
 */

import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { lint, formatters, resolveConfig, createPlugin } from "./index.mjs";

const require = createRequire(import.meta.url);

// Tests that lint from another directory need a binary path that does not
// depend on the working directory.
if (process.env.GALE_BINARY?.includes("/")) {
  process.env.GALE_BINARY = resolve(process.env.GALE_BINARY);
}

let passed = 0;
let failed = 0;
let current = "";

// Dots reporter: one "." per passing assertion, "F" per failure. Failure
// details are printed on their own line so the dots stay readable.
function section(name) {
  current = name;
}

function fail(message) {
  process.stdout.write("F");
  console.error(`\n  FAIL [${current}] ${message}`);
  failed++;
}

function assert(condition, message) {
  if (condition) {
    process.stdout.write(".");
    passed++;
  } else {
    fail(message);
  }
}

// Runs fn with console.warn muted, for APIs that warn by design.
function quietly(fn) {
  const warn = console.warn;
  console.warn = () => {};
  try {
    return fn();
  } finally {
    console.warn = warn;
  }
}

// ---------------------------------------------------------------------------
// Test 1: lint({ code }) with empty block
// ---------------------------------------------------------------------------

async function testLintCodeEmptyBlock() {
  section("Test 1: lint({ code: 'a {}' })");

  try {
    const result = await lint({ code: "a {}" });

    assert(result != null, "result is not null");
    assert(typeof result.cwd === "string", "result.cwd is a string");
    assert(Array.isArray(result.results), "result.results is an array");
    assert(typeof result.errored === "boolean", "result.errored is a boolean");
    assert(typeof result.report === "string", "result.report is a string");
    assert(typeof result.ruleMetadata === "object", "result.ruleMetadata is an object");

    if (result.results.length > 0) {
      const first = result.results[0];
      assert(typeof first.source === "string", "first result has source");
      assert(Array.isArray(first.warnings), "first result has warnings array");
      assert(Array.isArray(first.deprecations), "first result has deprecations array");
      assert(Array.isArray(first.parseErrors), "first result has parseErrors array");
      assert(typeof first.errored === "boolean", "first result has errored boolean");
      assert(typeof first.ignored === "boolean", "first result has ignored boolean");
    }
  } catch (err) {
    fail(`ERROR: ${err.message}`);
  }
}

// ---------------------------------------------------------------------------
// Test 2: lint({ code, config }) with color-named rule
// ---------------------------------------------------------------------------

async function testLintCodeWithConfig() {
  section("Test 2: lint({ code: 'a { color: pink; }', config: { rules: { 'color-named': 'never' } } })");

  try {
    const result = await lint({
      code: "a { color: pink; }",
      config: { rules: { "color-named": "never" } },
    });

    assert(result != null, "result is not null");
    assert(Array.isArray(result.results), "result.results is an array");

    if (result.results.length > 0) {
      const warnings = result.results[0].warnings;
      assert(Array.isArray(warnings), "warnings is an array");

      if (warnings.length > 0) {
        const w = warnings[0];
        assert(typeof w.line === "number", "warning has line number");
        assert(typeof w.column === "number", "warning has column number");
        assert(typeof w.rule === "string", "warning has rule name");
        assert(typeof w.severity === "string", "warning has severity");
        assert(typeof w.text === "string", "warning has text");
        assert(
          w.rule === "color-named",
          `warning rule is "color-named" (got "${w.rule}")`,
        );
      }
    }
  } catch (err) {
    fail(`ERROR: ${err.message}`);
  }
}

// ---------------------------------------------------------------------------
// Test 3: resolveConfig
// ---------------------------------------------------------------------------

async function testResolveConfig() {
  section("Test 3: resolveConfig('test.css')");

  try {
    const config = await resolveConfig("test.css");
    // May be undefined if no config file is found in the directory
    assert(
      config === undefined || typeof config === "object",
      "resolveConfig returns object or undefined",
    );
  } catch (err) {
    fail(`ERROR: ${err.message}`);
  }
}

// ---------------------------------------------------------------------------
// Test 4: formatters.json resolves to a function
// ---------------------------------------------------------------------------

async function testFormattersJson() {
  section("Test 4: formatters.json resolves to a function");

  try {
    const jsonFormatter = await formatters.json;
    assert(typeof jsonFormatter === "function", "formatters.json resolves to a function");

    // Test it works
    const output = await jsonFormatter(
      [
        {
          source: "test.css",
          warnings: [
            { line: 1, column: 1, rule: "test-rule", severity: "warning", text: "test" },
          ],
        },
      ],
      {},
    );
    assert(typeof output === "string", "formatter returns a string");

    const parsed = JSON.parse(output);
    assert(Array.isArray(parsed), "JSON formatter output is parseable as array");
  } catch (err) {
    fail(`ERROR: ${err.message}`);
  }
}

// ---------------------------------------------------------------------------
// Test 5: createPlugin stub
// ---------------------------------------------------------------------------

async function testCreatePlugin() {
  section("Test 5: createPlugin returns stub");

  const plugin = quietly(() => createPlugin("my-rule", () => {}));
  assert(plugin.ruleName === "my-rule", 'plugin.ruleName is "my-rule"');
  assert(typeof plugin.rule === "function", "plugin.rule is a function");
}

// ---------------------------------------------------------------------------
// Test 6: LinterResult shape
// ---------------------------------------------------------------------------

async function testLinterResultShape() {
  section("Test 6: LinterResult has correct shape");

  try {
    const result = await lint({ code: "a { color: red; }" });

    assert("cwd" in result, "result has cwd");
    assert("results" in result, "result has results");
    assert("errored" in result, "result has errored");
    assert("report" in result, "result has report");
    assert("ruleMetadata" in result, "result has ruleMetadata");
    assert("maxWarningsExceeded" in result, "result has maxWarningsExceeded key");
    assert("code" in result, "result has code key");
  } catch (err) {
    fail(`ERROR: ${err.message}`);
  }
}

// ---------------------------------------------------------------------------
// Test 7: CommonJS entry point bridges to the ESM implementation
// ---------------------------------------------------------------------------

async function testCommonJsEntry() {
  section("Test 7: require('./index.cjs') exposes the same API");

  try {
    const cjs = require("./index.cjs");

    assert(typeof cjs.lint === "function", "cjs.lint is a function");
    assert(typeof cjs.resolveConfig === "function", "cjs.resolveConfig is a function");
    assert(typeof cjs.createPlugin === "function", "cjs.createPlugin is a function");
    assert(cjs.default === cjs, "cjs.default points back at module.exports");

    const jsonFormatter = await cjs.formatters.json;
    assert(typeof jsonFormatter === "function", "cjs.formatters.json resolves to a function");

    const result = await cjs.lint({
      code: "a {}",
      config: { rules: { "block-no-empty": true } },
    });
    assert(Array.isArray(result.results), "cjs.lint returns results");
    assert(
      result.results[0]?.warnings[0]?.rule === "block-no-empty",
      "cjs.lint reports block-no-empty",
    );
  } catch (err) {
    fail(`ERROR: ${err.message}`);
  }
}

// ---------------------------------------------------------------------------
// Test 8: lint({ code, codeFilename: "x.vue" }) lints the style blocks
// ---------------------------------------------------------------------------

const VUE_CODE =
  '<template><p style="margin: 0px">x</p></template>\n<style>\n.a { color: #ffffff; }\n</style>\n';
const VUE_RULES = { "color-hex-length": "short", "length-zero-no-unit": true };

async function testLintVueCode() {
  section("Test 8: lint({ code, codeFilename: 'x.vue' })");

  try {
    const result = await lint({ code: VUE_CODE, codeFilename: "x.vue", config: { rules: VUE_RULES } });
    const warnings = result.results[0]?.warnings ?? [];
    const found = warnings.map((w) => `${w.line}:${w.column} ${w.rule}`);
    assert(
      JSON.stringify(found) === JSON.stringify(["1:30 length-zero-no-unit", "3:13 color-hex-length"]),
      `warnings sit at their place in the file (got ${JSON.stringify(found)})`,
    );

    const fixed = await lint({
      code: VUE_CODE,
      codeFilename: "x.vue",
      config: { rules: VUE_RULES },
      fix: true,
    });
    assert(
      fixed.code ===
        '<template><p style="margin: 0">x</p></template>\n<style>\n.a { color: #fff; }\n</style>\n',
      `fixed code is the whole file with its styles fixed (got ${JSON.stringify(fixed.code)})`,
    );
  } catch (err) {
    fail(`ERROR: ${err.message}`);
  }
}

// ---------------------------------------------------------------------------
// Test 9: lint({ files: ["**/*.vue"] }) finds and lints Vue files
// ---------------------------------------------------------------------------

async function testLintVueFiles() {
  section("Test 9: lint({ files: ['**/*.vue'] })");

  const dir = mkdtempSync(join(tmpdir(), "gale-api-"));
  try {
    writeFileSync(join(dir, "App.vue"), VUE_CODE);
    writeFileSync(join(dir, "notes.md"), "# not linted\n");
    const result = await lint({ files: ["**/*.vue"], cwd: dir, config: { rules: VUE_RULES } });

    assert(result.results.length === 1, `one file linted (got ${result.results.length})`);
    assert(result.results[0]?.source.endsWith("App.vue"), "the Vue file is the one linted");
    assert(result.results[0]?.warnings.length === 2, "both of its problems are reported");
    assert(result.errored === true, "the run is errored");
  } catch (err) {
    fail(`ERROR: ${err.message}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Test 10: sources are absolute paths, as in Stylelint
// ---------------------------------------------------------------------------

async function testSourcePaths() {
  section("Test 10: sources are absolute paths");

  const dir = mkdtempSync(join(tmpdir(), "gale-api-"));
  try {
    // The binary resolves paths from the working directory it runs in,
    // which is the real path of the temporary directory.
    const real = realpathSync(dir);
    mkdirSync(join(dir, "src"));
    writeFileSync(join(dir, "src", "a.css"), "a {}\n");
    const config = { rules: { "block-no-empty": true } };

    const fromCode = await lint({ code: "a {}\n", codeFilename: "./src/x.css", cwd: dir, config });
    assert(
      fromCode.results[0]?.source === join(real, "src", "x.css"),
      `codeFilename is resolved from cwd (got ${fromCode.results[0]?.source})`,
    );

    const fromFiles = await lint({ files: ["./src/*.css"], cwd: dir, config });
    assert(
      fromFiles.results[0]?.source === join(real, "src", "a.css"),
      `a glob reports absolute paths (got ${fromFiles.results[0]?.source})`,
    );

    const string = await formatters.string;
    const report = await string(fromFiles.results, { cwd: real });
    assert(report.startsWith("src/a.css\n"), `the string formatter names files from cwd (got ${report})`);
  } catch (err) {
    fail(`ERROR: ${err.message}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// Run all tests
// ---------------------------------------------------------------------------

async function main() {
  await testLintCodeEmptyBlock();
  await testLintCodeWithConfig();
  await testResolveConfig();
  await testFormattersJson();
  await testCreatePlugin();
  await testLinterResultShape();
  await testCommonJsEntry();
  await testLintVueCode();
  await testLintVueFiles();
  await testSourcePaths();

  console.log(`\n${passed} passed, ${failed} failed (Node ${process.versions.node})`);

  if (failed > 0) {
    process.exit(1);
  }
}

main();
