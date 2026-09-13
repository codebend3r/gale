/**
 * The npm package: TypeScript declarations, API result shape, and the
 * platform launcher that has to work on Windows.
 */

import { afterAll, describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { GALE_BIN, REPO_ROOT, cleanupProjects } from "./helpers";

afterAll(cleanupProjects);

// Point the wrapper at the binary under test rather than the bundled one.
process.env.GALE_BINARY = GALE_BIN;

const NPM_DIR = join(REPO_ROOT, "npm");

function readJson(rel: string): Record<string, unknown> {
  return JSON.parse(readFileSync(join(REPO_ROOT, rel), "utf8")) as Record<string, unknown>;
}

describe("TypeScript declarations", () => {
  const dts = join(NPM_DIR, "index.d.ts");

  test("npm/index.d.ts exists", () => {
    expect(existsSync(dts)).toBe(true);
  });

  test("declares the public API", () => {
    const text = readFileSync(dts, "utf8");

    expect(text).toMatch(/export (declare )?function lint\(/);
    expect(text).toMatch(/export (declare )?function resolveConfig\(/);
    expect(text).toMatch(/export (declare )?const formatters/);
    expect(text).toMatch(/export (declare )?function createPlugin\(/);
    expect(text).toMatch(/interface LinterResult/);
    expect(text).toMatch(/interface LintResult/);
    expect(text).toMatch(/interface Warning/);
  });

  test("the npm package.json points at the declarations", () => {
    const pkg = readJson("npm/package.json");
    const exports = pkg.exports as Record<string, Record<string, string>>;

    expect(pkg.types).toBe("index.d.ts");
    expect(exports["."].types).toBe("./index.d.ts");
    expect(pkg.files as string[]).toContain("index.d.ts");
  });

  test("the root package.json points at the declarations", () => {
    const pkg = readJson("package.json");
    const exports = pkg.exports as Record<string, Record<string, string>>;

    expect(pkg.types).toBe("npm/index.d.ts");
    expect(exports["."].types).toBe("./npm/index.d.ts");
  });
});

describe("lint() warning shape", () => {
  test("warnings carry line, column, rule, severity and text", async () => {
    const { lint } = await import("../../npm/index.mjs");

    const result = await lint({ code: "a {}", config: { rules: { "block-no-empty": true } } });
    const [warning] = result.results[0].warnings;

    expect(warning).toMatchObject({
      line: 1,
      column: 3,
      rule: "block-no-empty",
      severity: "error",
    });
    expect(warning.text).toContain("block-no-empty");
  });

  test("warnings carry endLine and endColumn", async () => {
    const { lint } = await import("../../npm/index.mjs");

    const result = await lint({ code: "a {}", config: { rules: { "block-no-empty": true } } });
    const [warning] = result.results[0].warnings;

    expect(warning.endLine).toBe(1);
    expect(typeof warning.endColumn).toBe("number");
  });

  test("warnings carry the configured url", async () => {
    const { lint } = await import("../../npm/index.mjs");

    const result = await lint({
      code: "a {}",
      config: { rules: { "block-no-empty": [true, { url: "https://example.com/empty" }] } },
    });
    const [warning] = result.results[0].warnings;

    expect(warning.url).toBe("https://example.com/empty");
  });
});

describe("platform launcher", () => {
  test("maps every supported platform to a Rust target", async () => {
    const { resolveTarget } = await import("../../npm/platform.cjs");

    expect(resolveTarget("darwin", "arm64")).toBe("aarch64-apple-darwin");
    expect(resolveTarget("darwin", "x64")).toBe("x86_64-apple-darwin");
    expect(resolveTarget("linux", "x64")).toBe("x86_64-unknown-linux-gnu");
    expect(resolveTarget("linux", "arm64")).toBe("aarch64-unknown-linux-gnu");
    expect(resolveTarget("win32", "x64")).toBe("x86_64-pc-windows-msvc");
    expect(resolveTarget("win32", "arm64")).toBe("aarch64-pc-windows-msvc");
    expect(resolveTarget("freebsd", "x64")).toBeNull();
  });

  test("names the Windows binary with an .exe suffix", async () => {
    const { binaryFileName } = await import("../../npm/platform.cjs");

    expect(binaryFileName("win32")).toBe("gale.exe");
    expect(binaryFileName("darwin")).toBe("gale");
    expect(binaryFileName("linux")).toBe("gale");
  });

  test("the bin entry is a Node script, not a POSIX shell script", () => {
    const pkg = readJson("npm/package.json");
    const bin = (pkg.bin as Record<string, string>).gale;

    expect(bin).toMatch(/\.c?js$/);
    const firstLine = readFileSync(join(NPM_DIR, bin), "utf8").split("\n")[0];
    expect(firstLine).toBe("#!/usr/bin/env node");
  });

  test("the root package.json uses the same launcher", () => {
    const pkg = readJson("package.json");
    const bin = (pkg.bin as Record<string, string>).gale;

    expect(bin).toMatch(/^npm\/bin\/gale\.c?js$/);
  });
});

describe("Windows release binaries", () => {
  const workflow = readFileSync(join(REPO_ROOT, ".github/workflows/release.yml"), "utf8");

  test("the release workflow builds x86_64-pc-windows-msvc", () => {
    expect(workflow).toContain("x86_64-pc-windows-msvc");
    expect(workflow).toMatch(/runs-on:.*windows/);
  });

  test("the Windows binary is staged into the npm package", () => {
    expect(workflow).toContain("npm/bin/x86_64-pc-windows-msvc/gale.exe");
  });
});
