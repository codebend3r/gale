import { afterAll, expect, test } from "bun:test";
import { chmodSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { cleanupProjects, config, makeProject, runGale } from "./helpers";

afterAll(cleanupProjects);

for (const cache of [false, true]) {
  test(`unreadable sources fail instead of disappearing (cache=${cache})`, () => {
    const project = makeProject({
      ".stylelintrc.json": config({ "block-no-empty": true }),
      // Invalid UTF-8 deterministically fails read_to_string, including as root.
      "a.css": "a {}",
    });
    writeFileSync(join(project.dir, "a.css"), new Uint8Array([0xff]));
    const result = runGale([...(cache ? ["--cache"] : []), "a.css"], { cwd: project.dir });
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("Error reading");
  });
}

test("failed autofix returns failure and leaves the file untouched", () => {
  const source = "a {color:#ffffff}";
  const project = makeProject({
    ".stylelintrc.json": config({ "color-hex-length": "short" }),
    "a.css": source,
  });
  const path = join(project.dir, "a.css");
  chmodSync(path, 0o444);
  try {
    const result = runGale(["--fix", "a.css"], { cwd: project.dir });
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain("Error writing");
    expect(readFileSync(path, "utf8")).toBe(source);
  } finally {
    chmodSync(path, 0o644);
  }
});
