/**
 * Shared helpers for the feature test suite.
 *
 * Every test drives the real `gale` binary the way a user would: a temporary
 * project directory on disk, a config file, CSS fixtures, and a subprocess.
 * The binary is located from `GALE_BIN`, then `target/release/gale`, then
 * `target/debug/gale`.
 */

import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { stripVTControlCharacters } from "node:util";

export const REPO_ROOT = resolve(import.meta.dir, "../..");

function locateBinary(): string {
  const candidates = [
    process.env.GALE_BIN,
    join(REPO_ROOT, "target", "release", "gale"),
    join(REPO_ROOT, "target", "debug", "gale"),
  ].filter((c): c is string => Boolean(c));

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    "gale binary not found. Run `cargo build --release` or set GALE_BIN. " +
      `Looked in: ${candidates.join(", ")}`,
  );
}

/** Absolute path of the binary under test. */
export const GALE_BIN = locateBinary();

/**
 * The environment every subprocess starts from.
 *
 * Deliberately minimal: no `CI`, `NO_COLOR`, or `FORCE_COLOR`, so a test that
 * cares about colour controls every input that decides it.
 */
export const BASE_ENV: Record<string, string> = {
  PATH: process.env.PATH ?? "",
  HOME: process.env.HOME ?? "",
  TERM: "xterm-256color",
};

// ---------------------------------------------------------------------------
// Stylelint-shaped JSON output
// ---------------------------------------------------------------------------

export interface Warning {
  line: number;
  column: number;
  endLine?: number;
  endColumn?: number;
  rule: string;
  severity: "error" | "warning";
  text: string;
  url?: string;
}

export interface SourceResult {
  source: string;
  deprecations: unknown[];
  invalidOptionWarnings: unknown[];
  parseErrors: unknown[];
  errored: boolean;
  warnings: Warning[];
}

// ---------------------------------------------------------------------------
// Running the binary
// ---------------------------------------------------------------------------

export interface RunOptions {
  cwd?: string;
  stdin?: string;
  env?: Record<string, string>;
}

export interface RunResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  /** Parse stdout as Stylelint-shaped JSON. */
  json(): SourceResult[];
  /** Every warning across every source, in output order. */
  warnings(): Warning[];
}

export function runGale(args: string[], opts: RunOptions = {}): RunResult {
  const proc = Bun.spawnSync([GALE_BIN, ...args], {
    cwd: opts.cwd,
    stdin: opts.stdin === undefined ? "ignore" : Buffer.from(opts.stdin),
    stdout: "pipe",
    stderr: "pipe",
    env: { ...BASE_ENV, ...(opts.env ?? {}) },
  });

  const stdout = proc.stdout.toString();
  const stderr = proc.stderr.toString();

  return {
    stdout,
    stderr,
    exitCode: proc.exitCode,
    json() {
      try {
        return JSON.parse(stdout) as SourceResult[];
      } catch (err) {
        throw new Error(
          `stdout is not JSON (${(err as Error).message}).\nstdout: ${stdout}\nstderr: ${stderr}`,
        );
      }
    },
    warnings() {
      return this.json().flatMap((r) => r.warnings);
    },
  };
}

/** Run with `--formatter json` prepended. */
export function runGaleJson(args: string[], opts: RunOptions = {}): RunResult {
  return runGale(["--formatter", "json", ...args], opts);
}

// ---------------------------------------------------------------------------
// Temporary projects
// ---------------------------------------------------------------------------

export interface Project {
  dir: string;
  path(rel: string): string;
  read(rel: string): string;
  exists(rel: string): boolean;
  write(rel: string, content: string): void;
  /** Bump the mtime without changing the content. */
  touch(rel: string): void;
  remove(): void;
}

const liveProjects: Project[] = [];

/**
 * Create a throwaway project directory populated with `files`.
 *
 * Keys are paths relative to the project root; nested directories are created
 * on demand. Call `cleanupProjects()` from `afterAll` to delete everything a
 * test file created.
 */
export function makeProject(files: Record<string, string>): Project {
  const dir = mkdtempSync(join(tmpdir(), "gale-feature-"));

  const project: Project = {
    dir,
    path: (rel) => join(dir, rel),
    read: (rel) => readFileSync(join(dir, rel), "utf8"),
    exists: (rel) => existsSync(join(dir, rel)),
    write(rel, content) {
      const full = join(dir, rel);
      mkdirSync(dirname(full), { recursive: true });
      writeFileSync(full, content);
    },
    touch(rel) {
      const later = new Date(Date.now() + 5_000);
      utimesSync(join(dir, rel), later, later);
    },
    remove() {
      rmSync(dir, { recursive: true, force: true });
    },
  };

  for (const [rel, content] of Object.entries(files)) {
    project.write(rel, content);
  }

  liveProjects.push(project);
  return project;
}

export function cleanupProjects(): void {
  for (const project of liveProjects.splice(0)) {
    project.remove();
  }
}

// ---------------------------------------------------------------------------
// Small assertions and fixtures
// ---------------------------------------------------------------------------

/** Serialise a Stylelint config object for a `.stylelintrc.json` fixture. */
export function config(rules: Record<string, unknown>, extra: Record<string, unknown> = {}): string {
  return JSON.stringify({ rules, ...extra }, null, 2);
}

export function hasAnsi(text: string): boolean {
  // eslint-disable-next-line no-control-regex
  return /\x1b\[[0-9;]*m/.test(text);
}

export function stripAnsi(text: string): string {
  return stripVTControlCharacters(text);
}

/** A block that trips `block-no-empty`. */
export const EMPTY_BLOCK = "a {}\n";

/** A declaration that trips `color-hex-case: lower` and is fixable. */
export const UPPER_HEX = "a { color: #FFF; }\n";
