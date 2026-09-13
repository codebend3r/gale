/**
 * Type declarations for @codebend3r/gale.
 *
 * The shapes mirror Stylelint's public API so a project can swap the import
 * without touching its call sites. Fields Gale never populates are typed as
 * they are in Stylelint but are always empty.
 */

export type Severity = "error" | "warning";

export type FormatterType = "string" | "json" | "compact" | "verbose" | "tap" | "unix";

/** A single problem reported for a source. */
export interface Warning {
  line: number;
  column: number;
  endLine?: number;
  endColumn?: number;
  rule: string;
  severity: Severity;
  /** The message with ` (rule-name)` appended, as Stylelint prints it. */
  text: string;
  /** Present when the rule's `url` secondary option is set. */
  url?: string;
}

/** The result of linting one source. */
export interface LintResult {
  source: string;
  warnings: Warning[];
  /** Always empty: Gale reports no deprecations. */
  deprecations: unknown[];
  /** Always empty: Gale does not validate rule options. */
  invalidOptionWarnings: unknown[];
  /** Always empty: parse errors surface as `parse-error` warnings. */
  parseErrors: unknown[];
  errored: boolean;
  ignored: boolean;
}

export interface MaxWarningsExceeded {
  maxWarnings: number;
  foundWarnings: number;
}

/** What `lint()` resolves to. */
export interface LinterResult {
  cwd: string;
  results: LintResult[];
  errored: boolean;
  /** The formatted report, in the formatter requested (JSON by default). */
  report: string;
  /** The fixed source when `fix` and `code` were both given. */
  code?: string;
  maxWarningsExceeded?: MaxWarningsExceeded;
  /** Always empty: Gale exposes no per-rule metadata. */
  ruleMetadata: Record<string, never>;
}

export type FormatterFunction = (
  results: LintResult[],
  returnValue: { cwd: string; results: LintResult[]; errored: boolean },
) => string;

/** Rule settings in any of Stylelint's accepted shapes. */
export type RuleSetting = boolean | null | string | number | unknown[];

export interface Config {
  extends?: string | string[];
  plugins?: string[];
  rules?: Record<string, RuleSetting>;
  overrides?: unknown[];
  ignoreFiles?: string | string[];
  ignorePatterns?: string | string[];
  customSyntax?: string;
  defaultSeverity?: Severity;
  ignoreDisables?: boolean;
  reportNeedlessDisables?: boolean;
  reportInvalidScopeDisables?: boolean;
  reportDescriptionlessDisables?: boolean;
  reportUnscopedDisables?: boolean;
  allowEmptyInput?: boolean;
  quiet?: boolean;
  fix?: boolean | "strict" | "lax";
  cache?: boolean;
  cacheLocation?: string;
  [key: string]: unknown;
}

export interface LinterOptions {
  /** Glob pattern(s) for files to lint. */
  files?: string | string[];
  /** CSS source to lint instead of files. */
  code?: string;
  /** Virtual filename for `code`, used for syntax detection. */
  codeFilename?: string;
  /** Inline config object. */
  config?: Config;
  /** Path to a config file. */
  configFile?: string;
  fix?: boolean | "strict" | "lax";
  formatter?: FormatterType | FormatterFunction;
  quiet?: boolean;
  cache?: boolean;
  cacheLocation?: string;
  maxWarnings?: number;
  allowEmptyInput?: boolean;
  ignorePath?: string;
  ignoreDisables?: boolean;
  reportNeedlessDisables?: boolean;
  reportInvalidScopeDisables?: boolean;
  reportDescriptionlessDisables?: boolean;
  /** Working directory; defaults to `process.cwd()`. */
  cwd?: string;
}

/** Lint files or a code string and resolve with a Stylelint-shaped result. */
export function lint(options?: LinterOptions): Promise<LinterResult>;

/** Resolve the effective config for a file, or `undefined` when none applies. */
export function resolveConfig(
  filePath: string,
  options?: { configFile?: string; cwd?: string },
): Promise<Config | undefined>;

/** Promise-based formatter functions keyed by name, like `stylelint.formatters`. */
export const formatters: {
  readonly json: Promise<FormatterFunction>;
  readonly string: Promise<FormatterFunction>;
  readonly compact: Promise<FormatterFunction>;
  readonly verbose: Promise<FormatterFunction>;
  readonly tap: Promise<FormatterFunction>;
  readonly unix: Promise<FormatterFunction>;
};

/**
 * Compatibility stub. Gale runs built-in Rust rules and cannot execute a
 * JavaScript rule; calling this warns and returns an inert plugin object.
 */
export function createPlugin(
  ruleName: string,
  ruleFunction: (...args: unknown[]) => unknown,
): { ruleName: string; rule: (...args: unknown[]) => unknown };

declare const _default: {
  lint: typeof lint;
  formatters: typeof formatters;
  resolveConfig: typeof resolveConfig;
  createPlugin: typeof createPlugin;
};

export default _default;
