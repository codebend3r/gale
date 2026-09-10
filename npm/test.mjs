#!/usr/bin/env node

/**
 * Basic smoke tests for the @codebend3r/gale programmatic API.
 *
 * Run: node test.mjs
 *
 * Requires a working gale binary (either in npm/bin/ or on PATH).
 */

import { createRequire } from "node:module";

import { lint, formatters, resolveConfig, createPlugin } from "./index.mjs";

const require = createRequire(import.meta.url);

let passed = 0;
let failed = 0;

function assert(condition, message) {
  if (condition) {
    console.log(`  PASS: ${message}`);
    passed++;
  } else {
    console.error(`  FAIL: ${message}`);
    failed++;
  }
}

// ---------------------------------------------------------------------------
// Test 1: lint({ code }) with empty block
// ---------------------------------------------------------------------------

async function testLintCodeEmptyBlock() {
  console.log("\nTest 1: lint({ code: 'a {}' })");

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
    console.error(`  ERROR: ${err.message}`);
    failed++;
  }
}

// ---------------------------------------------------------------------------
// Test 2: lint({ code, config }) with color-named rule
// ---------------------------------------------------------------------------

async function testLintCodeWithConfig() {
  console.log("\nTest 2: lint({ code: 'a { color: pink; }', config: { rules: { 'color-named': 'never' } } })");

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
      } else {
        console.log("  INFO: No warnings returned (gale may not flag this without config).");
      }
    }
  } catch (err) {
    console.error(`  ERROR: ${err.message}`);
    failed++;
  }
}

// ---------------------------------------------------------------------------
// Test 3: resolveConfig
// ---------------------------------------------------------------------------

async function testResolveConfig() {
  console.log("\nTest 3: resolveConfig('test.css')");

  try {
    const config = await resolveConfig("test.css");
    // May be undefined if no config file is found in the directory
    assert(
      config === undefined || typeof config === "object",
      "resolveConfig returns object or undefined",
    );
  } catch (err) {
    console.error(`  ERROR: ${err.message}`);
    failed++;
  }
}

// ---------------------------------------------------------------------------
// Test 4: formatters.json resolves to a function
// ---------------------------------------------------------------------------

async function testFormattersJson() {
  console.log("\nTest 4: formatters.json resolves to a function");

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
    console.error(`  ERROR: ${err.message}`);
    failed++;
  }
}

// ---------------------------------------------------------------------------
// Test 5: createPlugin stub
// ---------------------------------------------------------------------------

async function testCreatePlugin() {
  console.log("\nTest 5: createPlugin returns stub");

  const plugin = createPlugin("my-rule", () => {});
  assert(plugin.ruleName === "my-rule", 'plugin.ruleName is "my-rule"');
  assert(typeof plugin.rule === "function", "plugin.rule is a function");
}

// ---------------------------------------------------------------------------
// Test 6: LinterResult shape
// ---------------------------------------------------------------------------

async function testLinterResultShape() {
  console.log("\nTest 6: LinterResult has correct shape");

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
    console.error(`  ERROR: ${err.message}`);
    failed++;
  }
}

// ---------------------------------------------------------------------------
// Test 7: CommonJS entry point bridges to the ESM implementation
// ---------------------------------------------------------------------------

async function testCommonJsEntry() {
  console.log("\nTest 7: require('./index.cjs') exposes the same API");

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
    console.error(`  ERROR: ${err.message}`);
    failed++;
  }
}

// ---------------------------------------------------------------------------
// Run all tests
// ---------------------------------------------------------------------------

async function main() {
  console.log(`=== @codebend3r/gale programmatic API tests (Node ${process.versions.node}) ===`);

  await testLintCodeEmptyBlock();
  await testLintCodeWithConfig();
  await testResolveConfig();
  await testFormattersJson();
  await testCreatePlugin();
  await testLinterResultShape();
  await testCommonJsEntry();

  console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);

  if (failed > 0) {
    process.exit(1);
  }
}

main();
