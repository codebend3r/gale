/**
 * @codebend3r/gale — Stylelint-compatible programmatic API
 *
 * This module wraps the native Gale binary and exposes the same API surface
 * as `stylelint.lint()`, `stylelint.formatters`, and `stylelint.resolveConfig`.
 */

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, writeFileSync, unlinkSync, rmSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { tmpdir } from "node:os";
import { randomBytes } from "node:crypto";
import { fileURLToPath } from "node:url";

import { binaryFileName, resolveTarget } from "./platform.cjs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// ---------------------------------------------------------------------------
// Binary resolution
// ---------------------------------------------------------------------------

function findBinary() {
  // 0. An explicit override wins. Test suites and monorepos use this to point
  //    at a freshly built binary instead of the bundled one.
  const override = process.env.GALE_BINARY;
  if (override) {
    return override;
  }

  // 1. The binary bundled for this platform inside the npm package
  const target = resolveTarget(process.platform, process.arch);
  if (target) {
    const bundled = join(__dirname, "bin", target, binaryFileName(process.platform));
    if (existsSync(bundled)) {
      return bundled;
    }
  }

  // 2. Fall back to gale on PATH
  return "gale";
}

// ---------------------------------------------------------------------------
// Spawn helpers
// ---------------------------------------------------------------------------

function binaryNotFound(bin) {
  return new Error(
    `Gale binary not found. Looked for "${bin}". ` +
      "Install @codebend3r/gale or ensure gale is on your PATH.",
  );
}

/**
 * Spawn the gale binary and return { stdout, stderr, exitCode }.
 * If `stdinData` is provided it is piped to the process.
 */
function runGale(args, { stdinData, cwd } = {}) {
  return new Promise((resolve, reject) => {
    const bin = findBinary();
    const proc = spawn(bin, args, {
      cwd,
      stdio: ["pipe", "pipe", "pipe"],
      env: { ...process.env },
    });

    const stdoutChunks = [];
    const stderrChunks = [];

    proc.stdout.on("data", (chunk) => stdoutChunks.push(chunk));
    proc.stderr.on("data", (chunk) => stderrChunks.push(chunk));

    proc.on("error", (err) => {
      if (err.code === "ENOENT") {
        reject(binaryNotFound(bin));
      } else {
        reject(err);
      }
    });

    proc.on("close", (exitCode) => {
      resolve({
        stdout: Buffer.concat(stdoutChunks).toString("utf8"),
        stderr: Buffer.concat(stderrChunks).toString("utf8"),
        exitCode,
      });
    });

    if (stdinData != null) {
      proc.stdin.write(stdinData);
      proc.stdin.end();
    } else {
      proc.stdin.end();
    }
  });
}

/**
 * Format Stylelint-shaped results (a JSON string) with the binary's formatter
 * `name`, so the report is the one the CLI would print.  Runs in `cwd`
 * because the string and verbose formatters name files relative to it.
 */
function formatReport(name, resultsJson, cwd) {
  const bin = findBinary();
  const proc = spawnSync(bin, ["--format-results", name], {
    cwd,
    input: resultsJson,
    encoding: "utf8",
    maxBuffer: Infinity,
    env: { ...process.env },
  });
  if (proc.error) {
    throw proc.error.code === "ENOENT" ? binaryNotFound(bin) : proc.error;
  }
  if (proc.status !== 0) {
    throw new Error(proc.stderr.trim() || `gale --format-results ${name} exited ${proc.status}`);
  }
  return proc.stdout;
}

// ---------------------------------------------------------------------------
// Temp config helper
// ---------------------------------------------------------------------------

function writeTempConfig(config) {
  const dir = mkdtempSync(join(tmpdir(), "gale-"));
  const file = join(dir, "gale.json");
  writeFileSync(file, JSON.stringify(config, null, 2));
  return { file, dir };
}

function cleanupTempConfig({ file, dir }) {
  try {
    unlinkSync(file);
    rmSync(dir, { recursive: true, force: true });
  } catch {
    // best-effort cleanup
  }
}

// ---------------------------------------------------------------------------
// Parse Gale JSON output into Stylelint-shaped results
// ---------------------------------------------------------------------------

function parseJsonOutput(jsonString) {
  let raw;
  try {
    raw = JSON.parse(jsonString);
  } catch {
    return [];
  }

  if (!Array.isArray(raw)) return [];

  return raw.map((entry) => ({
    source: entry.source || "",
    warnings: (entry.warnings || []).map((w) => ({
      line: w.line,
      column: w.column,
      endLine: w.endLine,
      endColumn: w.endColumn,
      rule: w.rule,
      severity: w.severity || "warning",
      text: w.text,
      // Only present when the rule's `url` secondary option is set, as in
      // Stylelint, where an undefined url is dropped by JSON.stringify.
      ...(w.url !== undefined ? { url: w.url } : {}),
    })),
    deprecations: [],
    invalidOptionWarnings: (entry.invalidOptionWarnings || []).map((w) => ({ text: w.text })),
    parseErrors: [],
    errored:
      entry.errored ??
      ((entry.warnings || []).some((w) => w.severity === "error") ||
        (entry.invalidOptionWarnings || []).length > 0),
    ignored: false,
  }));
}

// ---------------------------------------------------------------------------
// lint()
// ---------------------------------------------------------------------------

/**
 * Lint CSS files or code, returning a Stylelint-compatible `LinterResult`.
 *
 * @param {object} options
 * @param {string|string[]} [options.files]        - Glob pattern(s) for files to lint (style sheets, or Vue, Svelte, Astro and HTML files)
 * @param {string}          [options.code]         - Source to lint instead of files
 * @param {string}          [options.codeFilename] - Virtual filename for `code`; `x.vue` lints the styles embedded in it
 * @param {object}          [options.config]       - Inline config object
 * @param {string}          [options.configFile]   - Path to config file
 * @param {boolean|string}  [options.fix]          - Enable autofix (true, "strict", or "lax")
 * @param {string|Function} [options.formatter]    - Formatter name or function
 * @param {boolean}         [options.quiet]        - Only report errors
 * @param {boolean}         [options.cache]        - Enable caching
 * @param {string}          [options.cacheLocation] - Override cache file location
 * @param {number}          [options.maxWarnings]  - Max warnings before erroring
 * @param {boolean}         [options.allowEmptyInput] - Don't error when no files match
 * @param {string}          [options.ignorePath]   - Path to a custom ignore file
 * @param {boolean}         [options.ignoreDisables] - Ignore all stylelint-disable comments
 * @param {boolean}         [options.reportNeedlessDisables] - Report needless disable comments
 * @param {boolean}         [options.reportInvalidScopeDisables] - Report invalid-scope disable comments
 * @param {boolean}         [options.reportDescriptionlessDisables] - Report descriptionless disable comments
 * @param {string}          [options.cwd]          - Working directory
 * @returns {Promise<LinterResult>}
 */
export async function lint(options = {}) {
  const cwd = options.cwd || process.cwd();
  const args = [];
  let tempConfig = null;

  try {
    // -- Formatter: always use JSON internally to get structured results --
    args.push("--formatter", "json");

    // -- Config --
    if (options.config) {
      tempConfig = writeTempConfig(options.config);
      args.push("--config", tempConfig.file);
    } else if (options.configFile) {
      args.push("--config", resolve(cwd, options.configFile));
    }

    // -- Fix --
    if (options.fix) {
      if (typeof options.fix === "string") {
        args.push(`--fix=${options.fix}`);
      } else {
        args.push("--fix");
      }
    }

    // -- Quiet --
    if (options.quiet) {
      args.push("--quiet");
    }

    // -- Cache --
    if (options.cache) {
      args.push("--cache");
    }

    // -- Max warnings --
    if (options.maxWarnings != null) {
      args.push("--max-warnings", String(options.maxWarnings));
    }

    // -- Cache location --
    if (options.cacheLocation) {
      args.push("--cache-location", resolve(cwd, options.cacheLocation));
    }

    // -- Allow empty input --
    if (options.allowEmptyInput) {
      args.push("--allow-empty-input");
    }

    // -- Ignore path --
    if (options.ignorePath) {
      args.push("--ignore-path", resolve(cwd, options.ignorePath));
    }

    // -- Ignore disables --
    if (options.ignoreDisables) {
      args.push("--ignore-disables");
    }

    // -- Report needless disables --
    if (options.reportNeedlessDisables) {
      args.push("--report-needless-disables");
    }

    // -- Report invalid scope disables --
    if (options.reportInvalidScopeDisables) {
      args.push("--report-invalid-scope-disables");
    }

    // -- Report descriptionless disables --
    if (options.reportDescriptionlessDisables) {
      args.push("--report-descriptionless-disables");
    }

    // -- Input source --
    let stdinData = null;

    if (options.code != null) {
      args.push("--stdin");
      if (options.codeFilename) {
        args.push("--stdin-filename", options.codeFilename);
      }
      stdinData = options.code;
    } else if (options.files) {
      const patterns = Array.isArray(options.files) ? options.files : [options.files];
      args.push(...patterns);
    } else {
      throw new Error(
        'Either "files" or "code" must be provided to lint().',
      );
    }

    const { stdout, stderr, exitCode } = await runGale(args, {
      stdinData,
      cwd,
    });

    // When allowEmptyInput is true and no files were found, return an
    // empty successful result instead of propagating any error.
    if (options.allowEmptyInput && !stdout.trim()) {
      return {
        cwd,
        results: [],
        errored: false,
        report: "",
        code: undefined,
        maxWarningsExceeded: undefined,
        ruleMetadata: {},
      };
    }

    // Parse the JSON results
    const results = parseJsonOutput(stdout);

    // Determine if any result had errors
    const errored = results.some((r) => r.errored);

    // Build formatted report if a specific formatter was requested, from
    // the JSON results this run already printed (none when the run failed)
    let report = stdout;
    if (
      options.formatter &&
      typeof options.formatter === "string" &&
      options.formatter !== "json"
    ) {
      report = stdout.trim() ? formatReport(options.formatter, stdout, cwd) : "";
    } else if (typeof options.formatter === "function") {
      report = options.formatter(results, { cwd, results, errored });
    }

    // Determine fixed code (only when fix + code input)
    let fixedCode;
    if (options.fix && options.code != null) {
      // When --fix + --stdin, gale outputs the fixed source to stdout.
      // We need to re-run with fix to get the fixed code, without JSON formatter.
      const fixArgs = [];
      if (tempConfig) {
        fixArgs.push("--config", tempConfig.file);
      } else if (options.configFile) {
        fixArgs.push("--config", resolve(cwd, options.configFile));
      }
      fixArgs.push("--fix", "--stdin");
      if (options.codeFilename) {
        fixArgs.push("--stdin-filename", options.codeFilename);
      }
      const fixResult = await runGale(fixArgs, { stdinData: options.code, cwd });
      fixedCode = fixResult.stdout;
    }

    // Max warnings check
    let maxWarningsExceeded;
    if (options.maxWarnings != null) {
      const totalWarnings = results.reduce(
        (sum, r) =>
          sum + r.warnings.filter((w) => w.severity === "warning").length,
        0,
      );
      if (totalWarnings > options.maxWarnings) {
        maxWarningsExceeded = {
          maxWarnings: options.maxWarnings,
          foundWarnings: totalWarnings,
        };
      }
    }

    return {
      cwd,
      results,
      errored,
      report,
      code: fixedCode,
      maxWarningsExceeded,
      ruleMetadata: {},
    };
  } finally {
    if (tempConfig) {
      cleanupTempConfig(tempConfig);
    }
  }
}

// ---------------------------------------------------------------------------
// formatters
// ---------------------------------------------------------------------------

/**
 * Create a formatter function that formats with the binary's `formatName`
 * formatter, the one `gale --formatter <formatName>` uses.
 */
function createFormatterFn(formatName) {
  return (results, returnValue) => {
    // Only the fields of Stylelint's results model; a Stylelint result also
    // carries internals such as `_postcssResult` that are no JSON.
    const model = results.map((r) => ({
      source: r.source,
      deprecations: r.deprecations,
      invalidOptionWarnings: r.invalidOptionWarnings,
      parseErrors: r.parseErrors,
      errored: r.errored,
      warnings: r.warnings,
    }));
    return formatReport(formatName, JSON.stringify(model), returnValue?.cwd || process.cwd());
  };
}

/**
 * Lazy promise-based formatters matching Stylelint's API.
 * Each getter returns a Promise<Function>.
 */
export const formatters = {
  get json() {
    return Promise.resolve(createFormatterFn("json"));
  },
  get string() {
    return Promise.resolve(createFormatterFn("string"));
  },
  get compact() {
    return Promise.resolve(createFormatterFn("compact"));
  },
  get verbose() {
    return Promise.resolve(createFormatterFn("verbose"));
  },
  get tap() {
    return Promise.resolve(createFormatterFn("tap"));
  },
  get unix() {
    return Promise.resolve(createFormatterFn("unix"));
  },
};

// ---------------------------------------------------------------------------
// resolveConfig()
// ---------------------------------------------------------------------------

/**
 * Resolve the effective config for a given file path.
 *
 * @param {string} filePath - The file to resolve config for
 * @param {object} [options]
 * @param {string} [options.configFile] - Explicit config file to use
 * @param {string} [options.cwd] - Working directory
 * @returns {Promise<object|undefined>}
 */
export async function resolveConfig(filePath, options = {}) {
  const cwd = options.cwd || process.cwd();
  const args = ["--print-config", filePath];

  if (options.configFile) {
    args.push("--config", resolve(cwd, options.configFile));
  }

  try {
    const { stdout, exitCode } = await runGale(args, { cwd });

    if (exitCode !== 0 || !stdout.trim()) {
      return undefined;
    }

    return JSON.parse(stdout);
  } catch {
    return undefined;
  }
}

// ---------------------------------------------------------------------------
// createPlugin() — compatibility stub
// ---------------------------------------------------------------------------

/**
 * Stub for Stylelint's `createPlugin()` API.
 * Gale uses built-in Rust rules instead of JS plugins.
 *
 * @param {string} ruleName
 * @param {Function} ruleFunction
 * @returns {{ ruleName: string, rule: Function }}
 */
export function createPlugin(ruleName, ruleFunction) {
  console.warn(
    `[gale] createPlugin("${ruleName}"): Gale uses built-in rules instead of JS plugins. ` +
      "This plugin will not be executed.",
  );
  return { ruleName, rule: ruleFunction };
}

// ---------------------------------------------------------------------------
// Default export (Stylelint compat)
// ---------------------------------------------------------------------------

export default {
  lint,
  formatters,
  resolveConfig,
  createPlugin,
};
