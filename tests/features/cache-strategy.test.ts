/**
 * `--cache-strategy`, matching Stylelint:
 *
 *   metadata  (default) a file is stale when its mtime or size changed
 *   content             a file is stale when its content hash changed
 *
 * A cache hit is observed through the debug log line Gale prints for it.
 */

import { afterAll, describe, expect, test } from "bun:test";

import { cleanupProjects, config, makeProject, runGale } from "./helpers";

afterAll(cleanupProjects);

const DEBUG = { GALE_LOG: "debug" };

function project() {
  return makeProject({
    ".stylelintrc.json": config({ "block-no-empty": true }),
    "a.css": "a { color: red; }\n",
  });
}

/** Whether a run reported a cache hit (the debug log may land on either stream). */
function hit(result: { stdout: string; stderr: string }): boolean {
  return /cache hit/i.test(result.stdout + result.stderr);
}

describe("--cache-strategy", () => {
  test("a second run with --cache hits the cache", () => {
    const { dir } = project();

    runGale(["--cache", "a.css"], { cwd: dir, env: DEBUG });
    const second = runGale(["--cache", "a.css"], { cwd: dir, env: DEBUG });

    expect(hit(second)).toBe(true);
  });

  test.failing("content: touching a file without changing it still hits", () => {
    const p = project();

    runGale(["--cache", "--cache-strategy", "content", "a.css"], { cwd: p.dir, env: DEBUG });
    p.touch("a.css");
    const result = runGale(["--cache", "--cache-strategy", "content", "a.css"], {
      cwd: p.dir,
      env: DEBUG,
    });

    expect(hit(result)).toBe(true);
  });

  test.failing("content: changing the content misses", () => {
    const p = project();

    runGale(["--cache", "--cache-strategy", "content", "a.css"], { cwd: p.dir, env: DEBUG });
    p.write("a.css", "a { color: blue; }\n");
    const result = runGale(["--cache", "--cache-strategy", "content", "a.css"], {
      cwd: p.dir,
      env: DEBUG,
    });

    expect(result.exitCode).toBe(0);
    expect(hit(result)).toBe(false);
  });

  test.failing("metadata: touching a file without changing it misses", () => {
    const p = project();

    runGale(["--cache", "--cache-strategy", "metadata", "a.css"], { cwd: p.dir, env: DEBUG });
    p.touch("a.css");
    const result = runGale(["--cache", "--cache-strategy", "metadata", "a.css"], {
      cwd: p.dir,
      env: DEBUG,
    });

    expect(result.exitCode).toBe(0);
    expect(hit(result)).toBe(false);
  });

  test.failing("metadata is the default strategy", () => {
    const p = project();

    runGale(["--cache", "a.css"], { cwd: p.dir, env: DEBUG });
    p.touch("a.css");
    const result = runGale(["--cache", "a.css"], { cwd: p.dir, env: DEBUG });

    expect(result.exitCode).toBe(0);
    expect(hit(result)).toBe(false);
  });

  test("cacheStrategy: content in the config keeps hits across a touch", () => {
    const p = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { cacheStrategy: "content" }),
      "a.css": "a { color: red; }\n",
    });

    runGale(["--cache", "a.css"], { cwd: p.dir, env: DEBUG });
    p.touch("a.css");
    const result = runGale(["--cache", "a.css"], { cwd: p.dir, env: DEBUG });

    expect(result.exitCode).toBe(0);
    expect(hit(result)).toBe(true);
  });

  test.failing("cacheStrategy: metadata in the config misses after a touch", () => {
    const p = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }, { cacheStrategy: "metadata" }),
      "a.css": "a { color: red; }\n",
    });

    runGale(["--cache", "a.css"], { cwd: p.dir, env: DEBUG });
    p.touch("a.css");
    const result = runGale(["--cache", "a.css"], { cwd: p.dir, env: DEBUG });

    expect(result.exitCode).toBe(0);
    expect(hit(result)).toBe(false);
  });

  test.failing("an unknown strategy is a usage error", () => {
    const { dir } = project();

    const result = runGale(["--cache", "--cache-strategy", "bogus", "a.css"], { cwd: dir });

    expect(result.exitCode).toBe(64);
    expect(result.stderr).toContain(
      '"bogus" cache strategy is unsupported. Specify either "metadata" or "content"',
    );
  });
});
