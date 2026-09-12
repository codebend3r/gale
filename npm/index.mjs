/**
 * @codebend3r/gale — Stylelint-compatible programmatic API
 *
 * This module wraps the native Gale binary and exposes the same API surface
 * as `stylelint.lint()`, `stylelint.formatters`, and `stylelint.resolveConfig`.
 */

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, writeFileSync, unlinkSync, rmSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { tmpdir } from "node:os";
import { randomBytes } from "node:crypto";
import { fileURLToPath } from "node:url";

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

  // 1. Check the bin/ directory within the npm package
  const localBin = join(__dirname, "bin", "gale");
  if (existsSync(localBin)) {
    return localBin;
  }

  // 2. Fall back to gale on PATH
  return "gale";
}

// ---------------------------------------------------------------------------
// Spawn helper
// ---------------------------------------------------------------------------

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
        reject(
          new Error(
            `Gale binary not found. Looked for "${bin}". ` +
              "Install @codebend3r/gale or ensure gale is on your PATH.",
          ),
        );
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
    invalidOptionWarnings: [],
    parseErrors: [],
    errored: (entry.warnings || []).some((w) => w.severity === "error"),
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
 * @param {string|string[]} [options.files]        - Glob pattern(s) for files to lint
 * @param {string}          [options.code]         - CSS code string to lint instead of files
 * @param {string}          [options.codeFilename] - Virtual filename for `code` (for syntax detection)
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

    // Build formatted report if a specific formatter was requested
    let report = stdout;
    if (
      options.formatter &&
      typeof options.formatter === "string" &&
      options.formatter !== "json"
    ) {
      // Re-run with the requested formatter for the report string
      const reportArgs = [...args];
      const jsonIdx = reportArgs.indexOf("json");
      if (jsonIdx !== -1) {
        reportArgs[jsonIdx] = options.formatter;
      }
      const reportResult = await runGale(reportArgs, { stdinData, cwd });
      report = reportResult.stdout;
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
 * Create a formatter function that calls gale with the given format name.
 */
function createFormatterFn(formatName) {
  return async (results, returnValue) => {
    // Build a minimal JSON array to pipe to gale for re-formatting.
    // Since gale reads files (not a JSON stream), we format client-side
    // for the simple built-in formats.
    if (formatName === "json") {
      return JSON.stringify(
        results.map((r) => ({
          source: r.source,
          warnings: r.warnings,
        })),
      );
    }

    // For other formatters, produce a simple text representation
    // matching what gale would output.
    let output = "";

    if (formatName === "compact") {
      for (const r of results) {
        for (const w of r.warnings) {
          output += `${r.source}: line ${w.line}, col ${w.column}, ${w.severity} - ${w.text}\n`;
        }
      }
      return output;
    }

    if (formatName === "tap") {
      output += "TAP version 13\n";
      output += `1..${results.length}\n`;
      results.forEach((r, i) => {
        if (r.warnings.length === 0) {
          output += `ok ${i + 1} - ${r.source}\n`;
        } else {
          output += `not ok ${i + 1} - ${r.source}\n`;
          for (const w of r.warnings) {
            output += `  ---\n`;
            output += `  message: "${w.text}"\n`;
            output += `  severity: ${w.severity}\n`;
            output += `  data:\n`;
            output += `    line: ${w.line}\n`;
            output += `    column: ${w.column}\n`;
            output += `    ruleId: ${w.rule}\n`;
            output += `  ...\n`;
          }
        }
      });
      return output;
    }

    if (formatName === "unix") {
      for (const r of results) {
        for (const w of r.warnings) {
          output += `${r.source}:${w.line}:${w.column}: ${w.text} [${w.severity}]\n`;
        }
      }
      const total = results.reduce((s, r) => s + r.warnings.length, 0);
      if (total > 0) {
        output += `\n${total} problem${total === 1 ? "" : "s"}\n`;
      }
      return output;
    }

    // "string" / "verbose" / default: human-readable
    for (const r of results) {
      if (r.warnings.length === 0) continue;
      output += `${r.source}\n`;
      for (const w of r.warnings) {
        const icon = w.severity === "error" ? "\u2716" : "\u26A0";
        output += `  ${w.line}:${w.column}  ${icon}  ${w.text}  ${w.rule}\n`;
      }
      output += "\n";
    }

    const totalErrors = results.reduce(
      (s, r) => s + r.warnings.filter((w) => w.severity === "error").length,
      0,
    );
    const totalWarnings = results.reduce(
      (s, r) => s + r.warnings.filter((w) => w.severity === "warning").length,
      0,
    );
    const total = totalErrors + totalWarnings;
    if (total > 0) {
      const p = total === 1 ? "problem" : "problems";
      const e = totalErrors === 1 ? "error" : "errors";
      const w = totalWarnings === 1 ? "warning" : "warnings";
      output += `\u2716 ${total} ${p} (${totalErrors} ${e}, ${totalWarnings} ${w})\n`;
    }

    return output;
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
