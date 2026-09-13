// CommonJS entry point.
//
// The implementation lives in index.mjs (ESM). CommonJS cannot `require()` an
// ES module synchronously, so this file bridges to it with a dynamic import.
// Every bridged function is already async, which makes the bridge transparent.
//
// This file also exists so that tools like `resolve-bin` and `require.resolve()`
// can locate the package.

let modulePromise;

function load() {
  if (!modulePromise) {
    modulePromise = import("./index.mjs");
  }
  return modulePromise;
}

/**
 * Lint CSS files or code. See index.mjs for the full option list.
 * @returns {Promise<object>} a Stylelint-compatible LinterResult
 */
async function lint(options) {
  return (await load()).lint(options);
}

/**
 * Resolve the effective config for a file path.
 * @returns {Promise<object|undefined>}
 */
async function resolveConfig(filePath, options) {
  return (await load()).resolveConfig(filePath, options);
}

/**
 * Stub for Stylelint's createPlugin(). Gale uses built-in Rust rules.
 * Kept synchronous to match Stylelint's signature.
 */
function createPlugin(ruleName, ruleFunction) {
  console.warn(
    `[gale] createPlugin("${ruleName}"): Gale uses built-in rules instead of JS plugins. ` +
      "This plugin will not be executed.",
  );
  return { ruleName, rule: ruleFunction };
}

// Stylelint exposes each formatter as a promise-returning getter.
const FORMATTER_NAMES = ["json", "string", "compact", "verbose", "tap", "unix"];
const formatters = {};
for (const name of FORMATTER_NAMES) {
  Object.defineProperty(formatters, name, {
    enumerable: true,
    get() {
      return load().then((m) => m.formatters[name]);
    },
  });
}

module.exports = { lint, resolveConfig, createPlugin, formatters };
module.exports.default = module.exports;
